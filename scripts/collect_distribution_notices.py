"""Collect local notice bytes and conservative Windows production dependency paths.

No downloads, legal conclusions, binary reachability claims or release approval.
Output is a fresh local review folder, never a completed shipping notice bundle.
"""
import argparse
import hashlib
import json
import subprocess
from pathlib import Path

from distribution_audit import notices


def digest(data):
    return hashlib.sha256(data).hexdigest()


def production_paths(metadata):
    """Keep normal/build edges, exclude dev edges; retain build-only provenance.

    Proc macros and their dependencies execute during the build. Normal paths
    are only potential runtime dependencies, not proof of linked machine code.
    """
    packages = {p["id"]: p for p in metadata["packages"]}
    nodes = {n["id"]: n for n in metadata["resolve"]["nodes"]}
    roots = [p for p in packages.values() if p["id"] in metadata["workspace_members"]
             and p["name"] in {"swarm-cli", "swarm-runtime", "swarm-desktop"}]
    if len(roots) != 3:
        raise ValueError("Expected CLI/runtime/desktop production roots")
    result = {}
    for root in roots:
        pending = [(root["id"], "normal", [root["name"]])]
        seen = set()
        while pending:
            package_id, role, path = pending.pop()
            if (package_id, role) in seen:
                continue
            seen.add((package_id, role))
            item = result.setdefault(package_id, {})
            item.setdefault(root["name"], {}).setdefault(role, path)
            for dep in nodes[package_id]["deps"]:
                package = packages[dep["pkg"]]
                macro = any("proc-macro" in t["kind"] for t in package["targets"])
                for kind in dep["dep_kinds"]:
                    if kind["kind"] == "dev":
                        continue
                    next_role = "build" if role == "build" or macro or kind["kind"] == "build" else "normal"
                    pending.append((dep["pkg"], next_role, path + [package["name"]]))
    return result


def copy_notices(directory, files, destination):
    directory = directory.resolve()
    records = []
    for filename in sorted(set(files)):
        source = Path(filename).resolve()
        if not source.is_relative_to(directory) or not source.is_file():
            raise ValueError("Notice outside package or missing")
        if source.stat().st_size > 4 * 1024 * 1024:
            raise ValueError("Notice exceeds review byte bound")
        data = source.read_bytes()
        relative = source.relative_to(directory)
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        with target.open("xb") as output:
            output.write(data)
        records.append({"package_path": relative.as_posix(), "bytes": len(data), "sha256": digest(data)})
    return records


def collect(root, output):
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--format-version", "1", "--locked", "--offline",
         "--filter-platform", "x86_64-pc-windows-msvc", "--features", "swarm-desktop/custom-protocol"],
        cwd=root, text=True, encoding="utf-8"))
    paths = production_paths(metadata)
    lock = json.loads((root / "apps/desktop/package-lock.json").read_text(encoding="utf-8"))
    output.mkdir(parents=True, exist_ok=False)
    entries = []
    for p in sorted(metadata["packages"], key=lambda p: (p["name"], p["version"], p["id"])):
        if p["id"] in metadata["workspace_members"] or p["id"] not in paths:
            continue
        directory = Path(p["manifest_path"]).parent
        files = notices(directory)
        if p.get("license_file"):
            files.append(str(directory / p["license_file"]))
        # ID hash distinguishes same-name/version packages from different sources.
        locator = f"cargo/{p['name']}-{p['version']}-{digest(p['id'].encode())[:12]}"
        records = copy_notices(directory, files, output / locator)
        entries.append({"ecosystem": "cargo", "name": p["name"], "version": p["version"],
                        "declared_license": p.get("license"), "production_paths": paths[p["id"]],
                        "notice_directory": locator, "notices": records})
    # Include every installed npm package as a conservative superset. The dev
    # flag cannot establish exclusion: build tools can copy bytes into bundles.
    for locator, p in sorted(lock["packages"].items()):
        if not locator:
            continue
        directory = root / "apps/desktop" / locator
        if not directory.resolve().is_relative_to((root / "apps/desktop/node_modules").resolve()):
            raise ValueError("npm locator outside node_modules")
        manifest = directory / "package.json"
        installed = json.loads(manifest.read_text(encoding="utf-8")) if manifest.is_file() else {}
        if installed and installed.get("version") != p.get("version"):
            raise ValueError("Installed npm version differs from lock")
        target = "npm/" + locator
        records = copy_notices(directory, notices(directory), output / target) if installed else []
        entries.append({"ecosystem": "npm", "name": installed.get("name", locator.rsplit("node_modules/", 1)[-1]),
                        "version": p.get("version"), "declared_license": installed.get("license", p.get("license")),
                        "installed": bool(installed), "dev": p.get("dev", False),
                        "scope": "all-lock-entries; installed notice superset, no bundle exclusion claim",
                        "notice_directory": target, "notices": records})
    report = {"schema_version": 1, "release_approved": False,
              "scope": "Windows x64 custom-protocol normal/build Cargo graph (no dev edges); all npm lock entries. Paths are potential dependencies, not linked/shipped proof. Exact local notice bytes only; missing texts and SPDX branch/attribution decisions remain open.",
              "lock_hashes": {"cargo": digest((root / "Cargo.lock").read_bytes()),
                              "npm": digest((root / "apps/desktop/package-lock.json").read_bytes())},
              "entries": entries,
              "gaps": [{"ecosystem": e["ecosystem"], "name": e["name"], "version": e["version"],
                        "installed": e.get("installed", True), "production_paths": e.get("production_paths", {})}
                       for e in entries if not e["notices"]],
              "remaining": ["Actual binary/frontend bundle reachability", "Missing upstream notice texts",
                            "Selected license branches and required attributions", "SQLite/WebView2/assets rights",
                            "Disposable Windows native/installer acceptance", "Explicit publication decision"]}
    with (output / "notice-review.json").open("x", encoding="utf-8", newline="\n") as file:
        json.dump(report, file, ensure_ascii=False, indent=2)
        file.write("\n")
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    output = args.output.resolve()
    if not output.is_relative_to(root / "target/distribution"):
        parser.error("Choose a fresh review folder under target/distribution")
    report = collect(root, output)
    print(json.dumps({"entries": len(report["entries"]), "notice_files": sum(len(e["notices"]) for e in report["entries"]),
                      "gaps": len(report["gaps"]), "release_approved": False}))
