"""Dependency graph and notice-byte boundaries; no legal/native acceptance."""
import tempfile
import unittest
from pathlib import Path

from collect_distribution_notices import copy_notices, digest, production_paths
from fetch_notice_supplements import pinned_sources


class NoticeBoundaries(unittest.TestCase):
    def test_paths_exclude_dev_and_track_build_and_macros(self):
        names = ["swarm-cli", "swarm-runtime", "swarm-desktop", "normal", "build", "macro", "macro-child", "dev"]
        packages = [{"id": n, "name": n, "targets": [{"kind": ["proc-macro" if n == "macro" else "lib"]}]} for n in names]
        def edge(name, kind=None):
            return {"pkg": name, "dep_kinds": [{"kind": kind}]}
        nodes = [{"id": n, "deps": []} for n in names]
        nodes[0]["deps"] = [edge("normal"), edge("build", "build"), edge("macro"), edge("dev", "dev")]
        nodes[4]["deps"] = [edge("normal")]
        nodes[5]["deps"] = [edge("macro-child")]
        nodes[6]["deps"] = [edge("macro")]  # cycles terminate without losing role
        result = production_paths({"packages": packages, "workspace_members": names[:3], "resolve": {"nodes": nodes}})
        self.assertNotIn("dev", result)
        self.assertEqual(set(result["normal"]["swarm-cli"]), {"normal", "build"})
        self.assertEqual(set(result["macro-child"]["swarm-cli"]), {"build"})
        self.assertIn("swarm-runtime", result)

    def test_exact_non_utf8_bytes_and_hash_are_preserved(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            package = root / "package"
            package.mkdir()
            source = package / "LICENSE"
            data = b"copyright\r\n\xff"
            source.write_bytes(data)
            output = root / "review"
            result = copy_notices(package, [str(source), str(source)], output)
            self.assertEqual((output / "LICENSE").read_bytes(), data)
            self.assertEqual(result, [{"package_path": "LICENSE", "bytes": len(data), "sha256": digest(data)}])
            with self.assertRaises(FileExistsError):
                copy_notices(package, [str(source)], output)

    def test_outside_package_rejected_without_copy(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            package = root / "package"
            package.mkdir()
            outside = root / "LICENSE"
            outside.write_bytes(b"private")
            output = root / "review"
            with self.assertRaises(ValueError):
                copy_notices(package, [str(outside)], output)
            self.assertFalse(output.exists())

    def test_oversize_notice_rejected_before_read(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            source = root / "LICENSE"
            with source.open("wb") as file:
                file.truncate(4 * 1024 * 1024 + 1)
            with self.assertRaises(ValueError):
                copy_notices(root, [str(source)], root / "review")

    def test_supplements_bind_repository_and_immutable_commit(self):
        import json
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            vcs = root / ".cargo_vcs_info.json"
            vcs.write_text(json.dumps({"git": {"sha1": "a" * 40}, "path_in_vcs": "alloc-stdlib"}), encoding="utf-8")
            package = {"name": "alloc-stdlib", "version": "0.3.0", "license": "BSD-3-Clause",
                       "repository": "https://github.com/dropbox/rust-alloc-no-stdlib", "manifest_path": str(root / "Cargo.toml")}
            record = pinned_sources(package)
            self.assertEqual(record["files"][0]["url"], "https://raw.githubusercontent.com/dropbox/rust-alloc-no-stdlib/" + "a" * 40 + "/LICENSE")
            package["repository"] = "https://example.com/foreign"
            with self.assertRaises(ValueError):
                pinned_sources(package)
            package["repository"] = "https://github.com/dropbox/rust-alloc-no-stdlib"
            vcs.write_text(json.dumps({"git": {"sha1": "main"}}), encoding="utf-8")
            with self.assertRaises(ValueError):
                pinned_sources(package)


if __name__ == "__main__":
    unittest.main()
