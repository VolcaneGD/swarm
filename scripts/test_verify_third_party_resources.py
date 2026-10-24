import json
import shutil
import tempfile
import unittest
from pathlib import Path
from verify_third_party_resources import verify


class ResourceVerificationTests(unittest.TestCase):
    def test_clean_source_inputs_and_tamper_are_verified_without_review_cache(self):
        project = Path(__file__).resolve().parents[1]
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in ["Cargo.lock", "apps/desktop/package-lock.json"]:
                target = root / name
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(project / name, target)
            directory = root / "distribution/third-party"
            shutil.copytree(project / "distribution/third-party", directory)
            self.assertEqual(len(verify(root)["covered_sources"]), 5)
            self.assertFalse((root / "target").exists())
            notices = directory / "THIRD_PARTY_NOTICES.txt"
            raw = notices.read_bytes()
            notices.write_bytes(b"x" + raw[1:])
            with self.assertRaisesRegex(ValueError, "hash drift"):
                verify(root)
            notices.write_bytes(raw)
            (directory / "private.txt").write_text("keep", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "Unknown/missing"):
                verify(root)
            (directory / "private.txt").unlink()
            manifest = directory / "bundle-manifest.json"
            document = json.loads(manifest.read_text())
            document["resources"][0]["path"] = "../foreign"
            manifest.write_text(json.dumps(document), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "Unsafe/duplicate"):
                verify(root)


if __name__ == "__main__":
    unittest.main()
