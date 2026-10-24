"""Retain shipping notices and append reviewed MIT/Apache dependency additions.

Local cache only. The existing covered MPL source set must remain unchanged.
Writes a fresh candidate, never overwrites evidence or publishes anything.
"""
import argparse
import json
import subprocess
from pathlib import Path
from distribution_audit import notices
from collect_distribution_notices import digest, production_paths
from verify_third_party_resources import verify


def refresh(root, output):
    root, output = root.resolve(), output.resolve()
    if not output.is_relative_to(root / "target/distribution") or output.exists():
        raise ValueError("Use a fresh target/distribution output")
    source = root / "distribution/third-party"
    manifest_path = source / "bundle-manifest.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    resources = {}
    for record in manifest["resources"]:
        name = record["path"]
        if Path(name).name != name:
            raise ValueError("Unsafe existing resource")
        path = source / name
        if path.is_symlink():
            raise ValueError("Linked resource")
        raw = path.read_bytes()
        if len(raw) != record["bytes"] or digest(raw) != record["sha256"]:
            raise ValueError("Existing resource drift")
        resources[name] = raw
    metadata = json.loads(subprocess.check_output([
        "cargo", "metadata", "--locked", "--offline", "--format-version", "1",
        "--filter-platform", "x86_64-pc-windows-msvc",
        "--features", "swarm-desktop/custom-protocol"], cwd=root, encoding="utf-8"))
    paths = production_paths(metadata)
    old = {(p["name"], p["version"]) for p in manifest["packages"] if p["ecosystem"] == "cargo"}
    covered = {(p["name"], p["version"]) for p in metadata["packages"]
               if p["id"] in paths and p.get("license") == "MPL-2.0"}
    if covered != {(p["name"], p["version"]) for p in manifest["covered_sources"]}:
        raise ValueError("Covered-source set changed; renewed source review required")
    additions, texts = [], []
    for package in sorted(metadata["packages"], key=lambda p: (p["name"], p["version"])):
        if package["id"] in metadata["workspace_members"] or package["id"] not in paths or (package["name"], package["version"]) in old:
            continue
        # This refresh deliberately supports only the newly reviewed license families.
        expression = package.get("license")
        if expression not in {"MIT", "Apache-2.0", "MIT OR Apache-2.0", "Apache-2.0 OR MIT"}:
            raise ValueError("New dependency needs license review: " + package["name"])
        directory = Path(package["manifest_path"]).parent
        found = notices(directory)
        if package.get("license_file"):
            declared = directory / package["license_file"]
            if declared.is_file() and declared.as_posix() not in found:
                found.append(declared.as_posix())
        if not found:
            raise ValueError("Missing dependency notices: " + package["name"])
        for file in found:
            raw = Path(file).read_bytes()
            heading = f"\n\n=== cargo {package['name']} {package['version']} / {Path(file).name} ===\n".encode()
            texts.extend([heading, raw, b"\n"])
        additions.append({"ecosystem": "cargo", "name": package["name"], "version": package["version"],
                          "declared_license": expression, "license_terms_retained": sorted(expression.split(" OR ")),
                          "scope": "conservative normal/build notice addition", "notice_files": len(found)})
    resources["THIRD_PARTY_NOTICES.txt"] += b"".join(texts)
    manifest["supplemental_reviews"] = [*manifest.get("supplemental_reviews", []), {
        "previous_manifest_sha256": digest(manifest_path.read_bytes()), "additions": additions}]
    manifest["packages"].extend(additions)
    manifest["notice_files"] += sum(p["notice_files"] for p in additions)
    manifest["lock_hashes"] = {"cargo": digest((root / "Cargo.lock").read_bytes()),
                               "npm": digest((root / "apps/desktop/package-lock.json").read_bytes())}
    manifest["resources"] = [{"path": name, "bytes": len(raw), "sha256": digest(raw)} for name, raw in sorted(resources.items())]
    manifest["release_approved"] = False
    output.mkdir(parents=True)
    for name, raw in resources.items():
        (output / name).write_bytes(raw)
    (output / "bundle-manifest.json").write_bytes((json.dumps(manifest, ensure_ascii=False, indent=2) + "\n").encode())
    verify(root, output)
    return additions


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    print(json.dumps({"additions": refresh(Path(__file__).resolve().parents[1], args.output), "release_approved": False}))
