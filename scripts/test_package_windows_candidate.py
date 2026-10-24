"""Maintainer boundary checks; these do not establish native installer acceptance."""
import hashlib
import json
import shutil
import struct
import tempfile
import unittest
from unittest.mock import patch
from pathlib import Path
from package_windows_candidate import document_link_gaps, mapped_entries, pe_info, stage


class CandidateBoundaries(unittest.TestCase):
    def test_actual_documents_have_no_unbundled_navigation(self):
        project = Path(__file__).resolve().parent.parent
        self.assertEqual(document_link_gaps(mapped_entries(project)), [])

    def test_missing_document_link_refuses_stage_before_output(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            config, resources = self.fixture(root)
            (root / "README.md").write_text("[Setup](docs/missing.md)", encoding="utf-8")
            resources["../../../README.md"] = "README.md"
            config.write_text(json.dumps({"bundle": {"resources": resources}}), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "Unbundled local document links"):
                stage(root, root / "stage")
            self.assertFalse((root / "stage").exists())

    def test_relative_fragment_encoded_and_reference_links(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            document = root / "guide.md"
            document.write_text("[Home](../README.md#start) [Space](<space%20name.md>) "
                                "[Web](https://example.org/) [Here](#section)\n"
                                "[Ref]: ../README.md\n"
                                "```md\n[Example](missing.md)\n```\n"
                                "`[Inline](missing.md)`\n[Escape](../../outside.md)", encoding="utf-8")
            entries = {"docs/guide.md": document, "README.md": document, "docs/space name.md": document}
            # Check one document's targets while other entries are non-Markdown files.
            other = root / "other.txt"
            other.write_text("", encoding="utf-8")
            entries["README.md"] = entries["docs/space name.md"] = other
            gaps = document_link_gaps(entries)
            self.assertEqual(len(gaps), 1)
            self.assertTrue(gaps[0]["outside_package"])

    def fixture(self, root):
        config = root / "apps/desktop/src-tauri"
        config.mkdir(parents=True)
        release = root / "target/release"
        release.mkdir(parents=True)
        for name in ["swarm-desktop.exe", "swarm.exe", "swarm-runtime.exe"]:
            (release / name).write_bytes(b"fixture-not-executed")
        (root / "LICENSE").write_text("fixture", encoding="utf-8")
        resources = {"../../../target/release/swarm.exe": "swarm.exe",
                     "../../../target/release/swarm-runtime.exe": "swarm-runtime.exe",
                     "../../../LICENSE": "LICENSE"}
        return config / "tauri.distribution.conf.json", resources

    def test_destination_traversal_fails_before_output_creation(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            config, resources = self.fixture(root)
            resources["../../../LICENSE"] = "../outside"
            config.write_text(json.dumps({"bundle": {"resources": resources}}), encoding="utf-8")
            output = root / "stage"
            with self.assertRaises(ValueError): stage(root, output)
            self.assertFalse(output.exists())
            self.assertFalse((root / "outside").exists())

    def test_missing_source_fails_without_partial_stage(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            config, resources = self.fixture(root)
            config.write_text(json.dumps({"bundle": {"resources": resources}}), encoding="utf-8")
            (root / "target/release/swarm-runtime.exe").unlink()
            output = root / "stage"
            with self.assertRaises(ValueError): stage(root, output)
            self.assertFalse(output.exists())

    def test_existing_stage_is_never_overwritten(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            config, resources = self.fixture(root)
            config.write_text(json.dumps({"bundle": {"resources": resources}}), encoding="utf-8")
            output = root / "stage"
            output.mkdir()
            sentinel = output / "user-file"
            sentinel.write_bytes(b"preserve")
            with self.assertRaises(FileExistsError): stage(root, output)
            self.assertEqual(sentinel.read_bytes(), b"preserve")
            self.assertEqual(len(list(output.iterdir())), 1)

    def test_malformed_pe_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "invalid.exe"
            for data in [b"not-PE", b"MZ", b"MZ" + bytes(100)]:
                path.write_bytes(data)
                with self.assertRaises((ValueError, struct.error)): pe_info(path)

    def test_installer_snapshot_survives_later_build_and_source_removal(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            config, resources = self.fixture(root)
            config.write_text(json.dumps({"bundle": {"resources": resources}}), encoding="utf-8")
            installer = root / "target/release/bundle/nsis/Swarm_0.1.0_x64-setup.exe"
            installer.parent.mkdir(parents=True)
            original = b"reviewed-installer-fixture"
            installer.write_bytes(original)
            def fixture_pe(path):
                return {"machine": "0x8664", "subsystem": 3 if path.name == "swarm.exe" else 2,
                        "imported_dlls": [], "resource_type_ids": [14]}
            with patch("package_windows_candidate.pe_info", side_effect=fixture_pe), patch("package_windows_candidate.subprocess.run"):
                report = stage(root, root / "stage")
            record = report["installers"][0]
            snapshot = root / record["path"]
            self.assertTrue(snapshot.is_relative_to(root / "stage/installers"))
            self.assertEqual(record["sha256"], hashlib.sha256(original).hexdigest())
            self.assertEqual(record["bytes"], len(original))
            installer.write_bytes(b"later-build")
            self.assertEqual(snapshot.read_bytes(), original)
            installer.unlink()
            self.assertEqual(snapshot.read_bytes(), original)
            self.assertNotIn("installers/Swarm_0.1.0_x64-setup.exe", {f["path"] for f in report["files"]})

    def test_installer_change_during_copy_refuses_report(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            config, resources = self.fixture(root)
            config.write_text(json.dumps({"bundle": {"resources": resources}}), encoding="utf-8")
            installer = root / "target/release/bundle/nsis/Swarm_0.1.0_x64-setup.exe"
            installer.parent.mkdir(parents=True)
            installer.write_bytes(b"before-copy")
            copyfile = shutil.copyfile
            def changing_copy(source, destination):
                copyfile(source, destination)
                if source == installer:
                    installer.write_bytes(b"concurrent-build")
            def fixture_pe(path):
                return {"machine": "0x8664", "subsystem": 3 if path.name == "swarm.exe" else 2,
                        "imported_dlls": [], "resource_type_ids": [14]}
            with patch("package_windows_candidate.pe_info", side_effect=fixture_pe), patch("package_windows_candidate.subprocess.run"), patch("package_windows_candidate.shutil.copyfile", side_effect=changing_copy):
                with self.assertRaisesRegex(ValueError, "Installer changed during staging"):
                    stage(root, root / "stage")
            self.assertFalse((root / "stage/package-report.json").exists())

    def test_user_and_ai_documents_preserve_relative_package_paths(self):
        project = Path(__file__).resolve().parent.parent
        actual_config = json.loads((project / "apps/desktop/src-tauri/tauri.distribution.conf.json").read_text(encoding="utf-8"))
        expected = ["docs/CLI_REFERENCE.md", "docs/SOURCE_REFERENCES.md", "docs/QUICKSTART.md", "docs/QUICKSTART.en.md",
                    "docs/USER_GUIDE.md", "docs/USER_GUIDE.en.md", "docs/STORAGE_LIFECYCLE.md", "docs/USE_CASES.md", "docs/USE_CASES.en.md",
                    "docs/AI_USAGE.md", "docs/AI_SETUP.md", "docs/RULES_GUIDE.md", "docs/MODEL_PROVENANCE.md",
                    "docs/cli-context.example.json", "docs/cli-body.example.md", "docs/rules-review.example.md",
                    ".agents/skills/swarm-cli/SKILL.md", ".agents/skills/swarm-cli/agents/openai.yaml"]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            config, resources = self.fixture(root)
            for destination in expected:
                source = "../../../" + destination
                self.assertEqual(actual_config["bundle"]["resources"].get(source), destination)
                content = (project / destination).read_bytes()
                target = root / destination
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(content)
                resources[source] = destination
            config.write_text(json.dumps({"bundle": {"resources": resources}}), encoding="utf-8")
            # PE/command/link-preflight mocks scope this to copying real bytes;
            # link readiness and executable/native acceptance are separate checks.
            def fixture_pe(path):
                return {"machine": "0x8664", "subsystem": 3 if path.name == "swarm.exe" else 2,
                        "imported_dlls": [], "resource_type_ids": [14]}
            with patch("package_windows_candidate.pe_info", side_effect=fixture_pe), patch("package_windows_candidate.subprocess.run"), patch("package_windows_candidate.document_link_gaps", return_value=[]):
                report = stage(root, root / "stage")
            self.assertFalse(report["release_approved"])
            for destination in expected:
                self.assertEqual((root / "stage" / destination).read_bytes(), (project / destination).read_bytes())


if __name__ == "__main__":
    unittest.main()
