"""Failure detection and read-only boundaries for acceptance bookkeeping."""
import contextlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import acceptance_status as cli


class AcceptanceStatusTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name).resolve()
        (self.root / "docs").mkdir()
        marker = "<!-- acceptance-status: checked=0 open=25 total=25 -->\n"
        for name in cli.DOCS:
            (self.root / name).write_text(marker, encoding="utf-8")
        rows = "".join(f"- [ ] Condition {i:02} remaining\n" for i in range(1, 26))
        with (self.root / cli.DOCS[0]).open("a", encoding="utf-8") as stream:
            stream.write(rows)
        queue = "<!-- acceptance-queue:start -->\n" + "".join(
            f"| R{i:02} | Condition {i:02} | human | H1 | human input |\n" for i in range(1, 26)
        ) + "<!-- acceptance-queue:end -->\n"
        with (self.root / "docs/REMAINING_WORK.md").open("a", encoding="utf-8") as stream:
            stream.write(queue)
        source = self.root / "crates/core/src/lib.rs"
        source.parent.mkdir(parents=True)
        source.write_text("// synthetic source\n", encoding="utf-8")

    def test_check_is_read_only_and_does_not_accept_product(self):
        before = cli.source_hashes(self.root)
        document_bytes = {name: (self.root / name).read_bytes() for name in cli.DOCS}
        result = cli.check(self.root)
        self.assertEqual(result["original_residuals"], {"closed": 0, "open": 25, "total": 25})
        self.assertEqual(result["acceptance"], "NOT_ASSESSED")
        self.assertFalse(result["publication_authorized"])
        self.assertEqual(document_bytes, {name: (self.root / name).read_bytes() for name in cli.DOCS})
        self.assertEqual(before, cli.source_hashes(self.root))

    def test_stale_counts_fail_without_rewriting(self):
        path = self.root / "PROJECT_SPEC.md"
        path.write_text("<!-- acceptance-status: checked=1 open=24 total=25 -->\n", encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "stale/missing"):
            cli.check(self.root)
        self.assertIn("checked=1", path.read_text(encoding="utf-8"))

    def test_new_open_row_cannot_disappear_from_queue(self):
        with (self.root / cli.DOCS[0]).open("a", encoding="utf-8") as stream:
            stream.write("- [ ] New unclassified request\n")
        with self.assertRaisesRegex(ValueError, "unclassified"):
            cli.check(self.root)

    def test_ambiguous_or_missing_residual_mapping_fails(self):
        path = self.root / "docs/REMAINING_WORK.md"
        original = path.read_text(encoding="utf-8")
        for locator in ("Condition", "Missing"):
            path.write_text(original.replace("Condition 01", locator), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "not unique"):
                cli.status(self.root)

    def test_invalid_utf8_and_replacement_character_fail(self):
        path = self.root / "docs/invalid.md"
        for data in (b"\xff", "\ufffd".encode()):
            path.write_bytes(data)
            with self.assertRaises(ValueError):
                cli.check(self.root)

    def test_missing_current_link_fails(self):
        path = self.root / "PROJECT_SPEC.md"
        with path.open("a", encoding="utf-8") as stream:
            stream.write("[missing](missing.md)\n<!-- acceptance-current:end -->\n")
        with self.assertRaisesRegex(ValueError, "missing/outside local link"):
            cli.check(self.root)

    def baseline(self):
        baseline = self.root / "before.json"
        baseline.write_text(json.dumps({"schema_version": 1, "root": str(self.root),
                                       "files": cli.source_hashes(self.root)}), encoding="utf-8")
        return baseline

    def test_source_added_changed_deleted_and_wrong_identity_fail(self):
        baseline = self.baseline()
        self.assertEqual(cli.verify_source(self.root, baseline), 1)
        path = self.root / "crates/core/src/lib.rs"
        path.write_text("changed", encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "source changed"):
            cli.verify_source(self.root, baseline)
        path.unlink()
        with self.assertRaises(ValueError):
            cli.verify_source(self.root, baseline)
        path.write_text("// synthetic source\n", encoding="utf-8")
        (path.parent / "added.rs").write_text("added", encoding="utf-8")
        with self.assertRaises(ValueError):
            cli.verify_source(self.root, baseline)
        saved = json.loads(baseline.read_text(encoding="utf-8"))
        saved["root"] = "another workspace"
        baseline.write_text(json.dumps(saved), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "identity"):
            cli.verify_source(self.root, baseline)

    def test_source_inventory_excludes_data_dependencies_and_build_output(self):
        for name in ("Data/private.json", "target/private.json", "apps/desktop/node_modules/private.json",
                     "apps/desktop/dist/private.json", "apps/desktop/.env"):
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"\xff")
        self.assertEqual(len(cli.source_hashes(self.root)), 1)
        cli.check(self.root)

    def test_snapshot_refuses_overwrite_and_bad_options(self):
        output = self.root / "protected.json"
        output.write_text("preserve", encoding="utf-8")
        with patch.object(cli, "ROOT", self.root), contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(cli.main(["snapshot", "--output", str(output)]), 1)
            self.assertEqual(cli.main(["check", "--output", str(output)]), 1)
        self.assertEqual(output.read_text(encoding="utf-8"), "preserve")

    def candidate(self):
        candidate = self.root / "target/distribution/candidate-test"
        candidate.mkdir(parents=True)
        (candidate / "app.exe").write_bytes(b"synthetic app")
        (candidate / "setup.exe").write_bytes(b"synthetic installer")
        report = {"candidate_only": True, "release_approved": False,
                  "files": [{"path": "app.exe", "sha256": cli.digest(candidate / "app.exe")}],
                  "installers": [{"path": "target/distribution/candidate-test/setup.exe",
                                  "sha256": cli.digest(candidate / "setup.exe")}]}
        return candidate, report

    def test_candidate_mutation_and_release_flags_fail(self):
        candidate, report = self.candidate()
        path = candidate / "package-report.json"
        path.write_text(json.dumps(report), encoding="utf-8")
        self.assertEqual(cli.verify_candidate(self.root, candidate), 2)
        (candidate / "app.exe").write_bytes(b"changed")
        with self.assertRaisesRegex(ValueError, "hash mismatch"):
            cli.verify_candidate(self.root, candidate)
        report["release_approved"] = True
        path.write_text(json.dumps(report), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "unpublished"):
            cli.verify_candidate(self.root, candidate)

    def test_manifest_path_traversal_and_duplicate_fail(self):
        for name in ("../private.json", "C:/private.json", "/private.json", "..\\private.json"):
            with self.assertRaises(ValueError):
                cli.safe_path(self.root, name)
        candidate, report = self.candidate()
        report["files"].append(report["files"][0])
        (candidate / "package-report.json").write_text(json.dumps(report), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "duplicate"):
            cli.verify_candidate(self.root, candidate)


if __name__ == "__main__":
    unittest.main()
