"""Prepare deterministic local third-party resources; never publish or approve release."""
import argparse
import json
import re
import subprocess
import tomllib
from pathlib import Path

from assemble_notice_review import contained, verified_bytes
from collect_distribution_notices import digest, production_paths
from prepare_selectors_source_review import verify_source


KNOWN_LICENSES = {"MIT", "MIT-0", "Apache-2.0", "ISC", "BSD-2-Clause", "BSD-3-Clause",
                  "0BSD", "Unlicense", "MPL-2.0", "Unicode-3.0", "Zlib", "CC0-1.0",
                  "BSL-1.0", "CDLA-Permissive-2.0", "CC-BY-4.0"}


def checked_json(path):
    if path.stat().st_size > 16 * 1024 * 1024:
        raise ValueError("Oversized review input")
    return json.loads(path.read_text(encoding="utf-8"))


def prepare(root, output, metadata=None):
    boundary = root / "target/distribution"
    output = contained(boundary, output)
    index_path = boundary / "notice-review-assembled-20261013/review-index.json"
    index = checked_json(index_path)
    locks = {"cargo": digest((root / "Cargo.lock").read_bytes()),
             "npm": digest((root / "apps/desktop/package-lock.json").read_bytes())}
    if index["lock_hashes"] != locks:
        raise ValueError("Notice review lock drift")
    local_path = contained(root, root / index["inputs"][0]["path"])
    if digest(local_path.read_bytes()) != index["inputs"][0]["sha256"]:
        raise ValueError("Notice input index drift")
    local = checked_json(local_path)
    unresolved = {(p["ecosystem"], p["name"], p["version"]) for p in index["unresolved_installed"]}
    if unresolved - {("cargo", "selectors", "0.38.0")}:
        raise ValueError("Unresolved package notice")
    decisions = []
    for package in local["entries"]:
        expression = package["declared_license"]
        if not isinstance(expression, str):
            raise ValueError("Missing declared license")
        leaves = set(re.findall(r"[A-Za-z0-9][A-Za-z0-9.-]*", expression)) - {"OR", "AND"}
        if not leaves or leaves - KNOWN_LICENSES:
            raise ValueError("License needs explicit review")
        decisions.append({"ecosystem": package["ecosystem"], "name": package["name"],
                          "version": package["version"], "declared_license": expression,
                          "license_terms_retained": sorted(leaves),
                          "scope": "conservative normal/build or installed npm superset" if package.get("installed", True)
                          else "not installed on this Windows host; no shipped exclusion inferred"})
    texts = [b"Swarm third-party license and attribution texts\n"
             b"Conservative build/runtime superset; all collected license branches retained.\n"
             b"Original notice bytes follow each package header; no endorsement is implied.\n\n"]
    for record in index["files"]:
        raw = verified_bytes(index_path.parent, index_path.parent / record["path"], record)
        header = (f"\n--- {record['ecosystem']} {record['name']} {record['version']} | "
                  f"{record['source_relative_path']} | {record['sha256']} ---\n").encode("utf-8")
        texts.extend([header, raw, b"\n"])
    source_review = checked_json(boundary / "selectors-source-review-20261013/source-review.json")
    if source_review["cargo_lock_sha256"] != locks["cargo"]:
        raise ValueError("MPL steward input lock drift")
    mpl = verified_bytes(boundary / "selectors-source-review-20261013",
                         boundary / "selectors-source-review-20261013/MPL-2.0.txt",
                         source_review["steward_license"])
    if metadata is None:
        metadata = json.loads(subprocess.check_output(
            ["cargo", "metadata", "--format-version", "1", "--locked", "--offline",
             "--filter-platform", "x86_64-pc-windows-msvc", "--features", "swarm-desktop/custom-protocol"],
            cwd=root, text=True, encoding="utf-8"))
    paths = production_paths(metadata)
    covered = [p for p in metadata["packages"] if p["id"] in paths and p.get("license") == "MPL-2.0"]
    expected = {("cssparser", "0.37.0"), ("cssparser-macros", "0.7.1"), ("dtoa-short", "0.3.5"),
                ("option-ext", "0.2.0"), ("selectors", "0.38.0")}
    if {(p["name"], p["version"]) for p in covered} != expected:
        raise ValueError("MPL package set needs renewed review")
    locked = tomllib.loads((root / "Cargo.lock").read_text(encoding="utf-8"))["package"]
    resources = {"THIRD_PARTY_NOTICES.txt": b"".join(texts), "MPL-2.0.txt": mpl}
    sources = []
    for package in sorted(covered, key=lambda p: p["name"]):
        source = Path(package["manifest_path"]).parent
        matches = [p for p in locked if p["name"] == package["name"] and p["version"] == package["version"]]
        if len(matches) != 1 or "checksum" not in matches[0]:
            raise ValueError("Ambiguous covered source")
        archive = source.parents[2] / "cache" / source.parent.name / (source.name + ".crate")
        raw, records = verify_source(archive, package, source, matches[0]["checksum"])
        filename = source.name + ".crate"
        resources[filename] = raw
        sources.append({"name": package["name"], "version": package["version"], "path": filename,
                        "sha256": digest(raw), "verified_files": len(records), "modified": False,
                        "potential_paths": paths[package["id"]]})
    source_notice = ["Swarm third-party source availability", "", "The following unmodified MPL-2.0 sources accompany this installation.",
                     "Each .crate file is a gzip tar source archive in this same third-party directory.",
                     "Extract with tar -xzf <filename>.crate. Original file notices are retained.",
                     "Covered source is available under MPL-2.0; see MPL-2.0.txt.",
                     "Swarm's MIT license does not replace third-party licenses.", ""]
    source_notice += [f"{p['name']} {p['version']}: {p['path']} (SHA256 {p['sha256']})" for p in sources]
    source_notice += ["", "SQLite amalgamation is public domain: https://www.sqlite.org/copyright.html",
                      "Microsoft WebView2 is separately licensed by Microsoft; the installer uses the official Evergreen Standalone Installer.",
                      "https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution", ""]
    resources["SOURCE_AVAILABILITY.txt"] = "\n".join(source_notice).encode("utf-8")
    report = {"schema_version": 1, "release_approved": False, "lock_hashes": locks,
              "notice_index_sha256": digest(index_path.read_bytes()), "notice_files": len(index["files"]),
              "license_policy": "Retain all collected license/NOTICE/copyright texts, including OR/slash alternatives; no branch text removed.",
              "packages": decisions, "covered_sources": sources,
              "resources": [{"path": name, "bytes": len(raw), "sha256": digest(raw)} for name, raw in sorted(resources.items())]}
    resources["bundle-manifest.json"] = (json.dumps(report, ensure_ascii=False, indent=2) + "\n").encode("utf-8")
    if output.exists():
        if {p.name for p in output.iterdir()} != set(resources):
            raise ValueError("Existing bundle has unknown/missing files")
        for name, raw in resources.items():
            path = contained(output, output / name)
            if (path.is_symlink() or not path.is_file()
                    or getattr(path.lstat(), "st_file_attributes", 0) & 0x400
                    or path.stat().st_size != len(raw) or path.read_bytes() != raw):
                raise ValueError("Existing bundle changed; preserve and review")
        return report
    output.mkdir(parents=True, exist_ok=False)
    for name, raw in resources.items():
        with (output / name).open("xb") as file:
            file.write(raw)
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=Path("target/distribution/third-party-bundle-current"))
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    result = prepare(root, args.output.resolve())
    print(json.dumps({"notice_files": result["notice_files"], "covered_sources": len(result["covered_sources"]),
                      "resources": len(result["resources"]), "release_approved": False}))
