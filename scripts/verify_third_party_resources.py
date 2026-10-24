"""Verify checked-in resource bytes/locks without original private review caches."""
import json
from pathlib import Path
from assemble_notice_review import contained
from collect_distribution_notices import digest


def verify(root, directory=None):
    directory = directory or root / "distribution/third-party"
    if directory.is_symlink():
        raise ValueError("Linked resource directory")
    manifest_path = directory / "bundle-manifest.json"
    if manifest_path.stat().st_size > 4 * 1024 * 1024:
        raise ValueError("Oversized resource manifest")
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    expected_locks = {"cargo": digest((root / "Cargo.lock").read_bytes()),
                      "npm": digest((root / "apps/desktop/package-lock.json").read_bytes())}
    if manifest["lock_hashes"] != expected_locks or manifest["release_approved"] is not False:
        raise ValueError("Resource lock/approval drift")
    records = manifest["resources"]
    names = [p["path"] for p in records]
    if len(names) != len(set(names)) or any(Path(name).name != name for name in names):
        raise ValueError("Unsafe/duplicate resource path")
    if {p.name for p in directory.iterdir()} != {*names, "bundle-manifest.json"}:
        raise ValueError("Unknown/missing resource file")
    for record in records:
        path = directory / record["path"]
        if path.is_symlink() or getattr(path.lstat(), "st_file_attributes", 0) & 0x400:
            raise ValueError("Linked resource file")
        path = contained(directory, path)
        if not path.is_file() or path.stat().st_size != record["bytes"] or record["bytes"] > 16 * 1024 * 1024:
            raise ValueError("Resource byte length drift")
        if digest(path.read_bytes()) != record["sha256"]:
            raise ValueError("Resource hash drift")
    return manifest


if __name__ == "__main__":
    root = Path(__file__).resolve().parents[1]
    result = verify(root)
    print(json.dumps({"resources": len(result["resources"]) + 1, "covered_sources": len(result["covered_sources"]),
                      "release_approved": False}))
