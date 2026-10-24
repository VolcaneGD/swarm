import tempfile
import unittest
from pathlib import Path
from export_source_candidate import export


class SourceExportTests(unittest.TestCase):
    def test_exact_source_private_record_omission_and_canonical_preservation(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ["Cargo.toml", "Cargo.lock", "LICENSE"]:
                (root / name).write_text("fixture", encoding="utf-8")
            code = root / "crates/core/src/lib.rs"
            code.parent.mkdir(parents=True)
            code.write_bytes(b"// Exact UTF-8 source\r\n")
            (root / "docs").mkdir()
            (root / "docs/HANDOVER.md").write_text("private-body-canary", encoding="utf-8")
            guide = root / "docs/GUIDE.md"
            guide.write_text(f"Root: {root}\n[Private](HANDOVER.md)\n", encoding="utf-8")
            original = guide.read_bytes()
            output = root / "target/distribution/source"
            report = export(root, output)
            self.assertFalse(report["release_approved"])
            self.assertEqual((output / code.relative_to(root)).read_bytes(), code.read_bytes())
            self.assertEqual(guide.read_bytes(), original)
            self.assertNotIn(str(root), (output / "docs/GUIDE.md").read_text())
            self.assertFalse((output / "docs/HANDOVER.md").exists())
            with self.assertRaises(ValueError): export(root, output)
            with self.assertRaises(ValueError): export(root, root / "outside")


if __name__ == "__main__":
    unittest.main()
