"""Review data/provenance boundaries, not legal or native acceptance."""
import json
import tempfile
import unittest
from pathlib import Path

from assemble_notice_review import assemble, digest


class ReviewBoundaries(unittest.TestCase):
    def fixture(self, root):
        (root / "apps/desktop").mkdir(parents=True)
        (root / "Cargo.lock").write_bytes(b"cargo")
        (root / "apps/desktop/package-lock.json").write_bytes(b"npm")
        folder = root / "target/distribution/input"
        folder.mkdir(parents=True)
        raw = b"copyright\r\n\xff"
        (folder / "LICENSE").write_bytes(raw)
        entry = {"ecosystem": "cargo", "name": "fixture", "version": "1", "declared_license": "MIT",
                 "notice_directory": ".", "notices": []}
        record = {"bytes": len(raw), "sha256": digest(raw), "local_path": "LICENSE",
                  "url": "https://raw.githubusercontent.com/test/test/" + "a" * 40 + "/LICENSE"}
        local = {"lock_hashes": {"cargo": digest(b"cargo"), "npm": digest(b"npm")}, "entries": [entry]}
        cargo = {"cargo_lock_sha256": digest(b"cargo"), "packages": [
            {"name": "fixture", "version": "1", "declared_license": "MIT", "commit": "a" * 40, "files": [record]}]}
        npm = {"lock_sha256": digest(b"npm"), "packages": []}
        paths = [folder / name for name in ("local.json", "cargo.json", "npm.json")]
        for path, report in zip(paths, (local, cargo, npm)):
            path.write_text(json.dumps(report), encoding="utf-8")
        return paths, root / "target/distribution/result", (local, cargo, npm), raw

    def test_exact_bytes_candidate_origin_and_no_approval(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            paths, output, reports, raw = self.fixture(root)
            result = assemble(root, *paths, output)
            self.assertFalse(result["release_approved"])
            self.assertEqual(result["files"][0]["origin"], "upstream_candidate")
            self.assertEqual((output / result["files"][0]["path"]).read_bytes(), raw)
            with self.assertRaises(ValueError):
                assemble(root, *paths, output)

    def test_tamper_lock_version_and_moving_ref_rejected_before_output(self):
        for mutation in ("bytes", "lock", "version", "commit", "escape"):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as temp:
                root = Path(temp)
                paths, output, reports, raw = self.fixture(root)
                if mutation == "bytes":
                    (paths[0].parent / "LICENSE").write_bytes(b"tampered")
                elif mutation == "lock":
                    (root / "Cargo.lock").write_bytes(b"changed")
                else:
                    package = reports[1]["packages"][0]
                    if mutation == "version":
                        package["version"] = "2"
                    elif mutation == "commit":
                        package["commit"] = "main"
                    else:
                        package["files"][0]["local_path"] = "../../../Cargo.lock"
                    paths[1].write_text(json.dumps(reports[1]), encoding="utf-8")
                with self.assertRaises(ValueError):
                    assemble(root, *paths, output)
                self.assertFalse(output.exists())

    def test_missing_installed_and_uninstalled_are_separate(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            paths, output, reports, raw = self.fixture(root)
            reports[1]["packages"] = []
            reports[0]["entries"].append({"ecosystem": "npm", "name": "other-platform", "version": "1",
                                           "declared_license": "MIT", "installed": False, "notices": []})
            for path, report in zip(paths, reports):
                path.write_text(json.dumps(report), encoding="utf-8")
            result = assemble(root, *paths, output)
            self.assertEqual([e["name"] for e in result["unresolved_installed"]], ["fixture"])
            self.assertEqual([e["name"] for e in result["uninstalled_without_collection"]], ["other-platform"])


if __name__ == "__main__":
    unittest.main()
