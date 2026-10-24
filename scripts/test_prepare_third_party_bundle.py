"""Actual cached notice/source integration with isolated tamper fixtures."""
import json
import shutil
import tempfile
import unittest
from pathlib import Path

from prepare_third_party_bundle import prepare


class ThirdPartyBundleTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.project = Path(__file__).resolve().parents[1]
        if not (cls.project / "target/distribution/metadata-notices-20261013.json").is_file():
            raise unittest.SkipTest("Maintainer's immutable notice review inputs are not included in clean source; shipped-resource tests remain runnable")
        cls.metadata = json.loads((cls.project / "target/distribution/metadata-notices-20261013.json").read_text(encoding="utf-8"))

    def fixture(self, directory):
        root = Path(directory)
        for name in ["Cargo.lock", "apps/desktop/package-lock.json"]:
            target = root / name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(self.project / name, target)
        for name in ["notice-review-assembled-20261013", "selectors-source-review-20261013"]:
            shutil.copytree(self.project / "target/distribution" / name, root / "target/distribution" / name)
        # Only the prior local index is needed; its notice bytes are in assembly.
        source = self.project / "target/distribution/notice-review-20261012-resume/notice-review.json"
        target = root / source.relative_to(self.project)
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)
        return root, root / "target/distribution/bundle"

    def test_actual_archives_notices_and_idempotent_byte_verification(self):
        with tempfile.TemporaryDirectory() as directory:
            root, output = self.fixture(directory)
            result = prepare(root, output, self.metadata)
            self.assertEqual(result["notice_files"], 813)
            self.assertEqual(len(result["covered_sources"]), 5)
            self.assertEqual(result, prepare(root, output, self.metadata))
            self.assertFalse(result["release_approved"])
            self.assertNotIn(str(self.project), json.dumps(result))
            self.assertNotIn(str(root), json.dumps(result))
            self.assertIn(b"selectors 0.38.0", (output / "SOURCE_AVAILABILITY.txt").read_bytes())
            (output / "MPL-2.0.txt").write_bytes(b"tampered")
            with self.assertRaisesRegex(ValueError, "Existing bundle changed"):
                prepare(root, output, self.metadata)
            self.assertEqual((output / "MPL-2.0.txt").read_bytes(), b"tampered")

    def test_input_tamper_lock_drift_and_unknown_file_fail_closed(self):
        for case in ["notice", "lock", "unknown"]:
            with self.subTest(case=case), tempfile.TemporaryDirectory() as directory:
                root, output = self.fixture(directory)
                if case == "notice":
                    index = json.loads((root / "target/distribution/notice-review-assembled-20261013/review-index.json").read_text())
                    (root / "target/distribution/notice-review-assembled-20261013" / index["files"][0]["path"]).write_bytes(b"tampered")
                elif case == "lock":
                    (root / "Cargo.lock").write_text("changed", encoding="utf-8")
                else:
                    prepare(root, output, self.metadata)
                    (output / "unknown.txt").write_text("keep", encoding="utf-8")
                with self.assertRaises(ValueError):
                    prepare(root, output, self.metadata)
                if case != "unknown":
                    self.assertFalse(output.exists())
                else:
                    self.assertEqual((output / "unknown.txt").read_text(), "keep")


if __name__ == "__main__":
    unittest.main()
