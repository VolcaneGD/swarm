"""Assemble hash-verified local/upstream notice candidates; never release clearance.

No network, installation, lock changes or automatic SPDX/attribution decisions.
Input and exclusive output folders must remain under target/distribution.
"""
import argparse
import json
import re
from pathlib import Path

from collect_distribution_notices import digest


def contained(base, path):
    resolved = path.resolve()
    if not resolved.is_relative_to(base.resolve()):
        raise ValueError("Review path escapes its owned directory")
    return resolved


def verified_bytes(base, path, record):
    source = contained(base, path)
    if not source.is_file() or source.stat().st_size > 4 * 1024 * 1024:
        raise ValueError("Missing or oversized notice")
    raw = source.read_bytes()
    if len(raw) != record["bytes"] or digest(raw) != record["sha256"]:
        raise ValueError("Notice byte/hash mismatch")
    return raw


def assemble(root, local_path, cargo_path, npm_path, output):
    boundary = root / "target/distribution"
    inputs = [contained(boundary, p) for p in (local_path, cargo_path, npm_path)]
    output = contained(boundary, output)
    if output.exists():
        raise ValueError("Preserve existing review output")
    local, cargo, npm = [json.loads(p.read_text(encoding="utf-8")) for p in inputs]
    locks = {"cargo": digest((root / "Cargo.lock").read_bytes()),
             "npm": digest((root / "apps/desktop/package-lock.json").read_bytes())}
    if (local["lock_hashes"] != locks or cargo["cargo_lock_sha256"] != locks["cargo"]
            or npm["lock_sha256"] != locks["npm"]):
        raise ValueError("Review/lock identity drift")
    entries = {(e["ecosystem"], e["name"], e["version"]): e for e in local["entries"]}
    if len(entries) != len(local["entries"]):
        raise ValueError("Duplicate package identity")
    files, indexed = [], []
    for key, entry in entries.items():
        for record in entry["notices"]:
            path = inputs[0].parent / entry["notice_directory"] / record["package_path"]
            files.append((key, "local", path, record, inputs[0].parent, None))
    for ecosystem, report, report_path in [("cargo", cargo, inputs[1]), ("npm", npm, inputs[2])]:
        for package in report["packages"]:
            key = (ecosystem, package["name"], package["version"])
            if key not in entries or package["declared_license"] != entries[key]["declared_license"]:
                raise ValueError("Supplement package/version/license mismatch")
            if not entries[key].get("installed", True):
                raise ValueError("Supplement does not identify an installed package")
            commit = package["commit"]
            if not re.fullmatch(r"[0-9a-f]{40}", commit):
                raise ValueError("Supplement requires an immutable commit")
            for record in package["files"]:
                if not record["url"].startswith("https://raw.githubusercontent.com/") or f"/{commit}/" not in record["url"]:
                    raise ValueError("Supplement source is not pinned")
                path = (report_path.parent / record["local_path"] if ecosystem == "cargo"
                        else Path(record["path"]))
                files.append((key, "upstream_candidate", path, record, report_path.parent,
                              {"url": record["url"], "commit": commit,
                               "provenance": package.get("provenance", "installed_crate_commit")}))
    # Verify every input and destination before creating even a partial output.
    prepared = []
    for index, (key, origin, path, record, base, provenance) in enumerate(files):
        raw = verified_bytes(base, path, record)
        filename = f"texts/{index:04d}-{origin}.txt"
        prepared.append((filename, raw))
        indexed.append({"ecosystem": key[0], "name": key[1], "version": key[2],
                        "origin": origin, "path": filename, "bytes": len(raw),
                        "sha256": digest(raw), "source": provenance,
                        "source_relative_path": path.resolve().relative_to(base.resolve()).as_posix()})
    covered = {(f["ecosystem"], f["name"], f["version"]) for f in indexed}
    unresolved = [dict(ecosystem=k[0], name=k[1], version=k[2],
                       declared_license=e["declared_license"])
                  for k, e in entries.items() if e.get("installed", True) and k not in covered]
    excluded = [dict(ecosystem=k[0], name=k[1], version=k[2])
                for k, e in entries.items() if not e.get("installed", True)]
    result = {"schema_version": 1, "release_approved": False, "lock_hashes": locks,
              "scope": "Conservative notice review superset; upstream files remain candidates. No license branch or shipping applicability approval.",
              "inputs": [{"path": p.relative_to(root).as_posix(), "sha256": digest(p.read_bytes())} for p in inputs],
              "files": indexed, "unresolved_installed": unresolved,
              "uninstalled_without_collection": excluded,
              "remaining": ["selectors MPL text/source availability",
                            "SPDX branches and copyright/NOTICE/attribution obligations",
                            "Actual shipped reachability, generated code, SQLite/WebView2/assets",
                            "Final notice bundle integration and explicit publication decision"]}
    output.mkdir(parents=True, exist_ok=False)
    (output / "texts").mkdir()
    for filename, raw in prepared:
        with (output / filename).open("xb") as file:
            file.write(raw)
    with (output / "review-index.json").open("x", encoding="utf-8", newline="\n") as file:
        json.dump(result, file, ensure_ascii=False, indent=2)
        file.write("\n")
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("local", "cargo", "npm", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    args = parser.parse_args()
    result = assemble(Path(__file__).resolve().parents[1], args.local, args.cargo, args.npm, args.output)
    print(json.dumps({"files": len(result["files"]), "unresolved_installed": result["unresolved_installed"],
                      "uninstalled": len(result["uninstalled_without_collection"]), "release_approved": False}))
