import io
import tarfile
import tempfile
import unittest
from pathlib import Path

from prepare_selectors_source_review import digest, verify_source


class SourceBoundaries(unittest.TestCase):
    def test_exact_locked_archive_and_installed_bytes(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            source = root / "source"
            source.mkdir()
            data = b"// Mozilla Public License, v. 2.0\n"
            (source / "lib.rs").write_bytes(data)
            archive = root / "source.crate"
            with tarfile.open(archive, "w:gz") as tar:
                member = tarfile.TarInfo("selectors-0.38.0/lib.rs")
                member.size = len(data)
                tar.addfile(member, io.BytesIO(data))
            checksum = digest(archive.read_bytes())
            package = {"name": "selectors", "version": "0.38.0"}
            raw, records = verify_source(archive, package, source, checksum)
            self.assertTrue(records[0]["mpl_header"])
            with self.assertRaises(ValueError):
                verify_source(archive, package, source, "0" * 64)
            (source / "lib.rs").write_bytes(b"modified")
            with self.assertRaises(ValueError):
                verify_source(archive, package, source, checksum)

    def test_traversal_member_rejected_without_extraction(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            source = root / "source"
            source.mkdir()
            archive = root / "source.crate"
            with tarfile.open(archive, "w:gz") as tar:
                member = tarfile.TarInfo("selectors-0.38.0/../outside")
                member.size = 1
                tar.addfile(member, io.BytesIO(b"x"))
            with self.assertRaises(ValueError):
                verify_source(archive, {"name": "selectors", "version": "0.38.0"}, source, digest(archive.read_bytes()))
            self.assertFalse((root / "outside").exists())


if __name__ == "__main__":
    unittest.main()
