"""Read local locked dependency metadata and produce a conservative release audit.

No installation, network access, bundling or publication is performed. Metadata
and located notice files are evidence to review, not legal/release approval.
"""

import argparse
import datetime
import json
import subprocess
from pathlib import Path


def notices(directory):
    result = []
    for path in sorted(directory.iterdir()):
        if not path.is_file():
            continue
        if path.name.lower().startswith(("license", "licence", "copying", "notice")):
            result.append(path.as_posix())
    return result


def collect(root):
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--format-version", "1", "--locked", "--offline",
         "--filter-platform", "x86_64-pc-windows-msvc"],
        cwd=root, text=True, encoding="utf-8",
    ))
    workspace = set(metadata["workspace_members"])
    rust = []
    for package in metadata["packages"]:
        if package["id"] in workspace:
            continue
        directory = Path(package["manifest_path"]).parent
        found = notices(directory)
        if package.get("license_file"):
            path = directory / package["license_file"]
            if path.is_file() and path.as_posix() not in found:
                found.append(path.as_posix())
        rust.append({
            "name": package["name"], "version": package["version"],
            "declared_license": package.get("license"), "notice_files": found,
            "scope": "cargo-metadata-windows-x64-default-workspace-features",
        })
    npm = []
    lock = json.loads((root / "apps/desktop/package-lock.json").read_text(encoding="utf-8"))
    for locator, package in lock["packages"].items():
        if not locator:
            continue
        directory = root / "apps/desktop" / locator
        manifest = directory / "package.json"
        installed = json.loads(manifest.read_text(encoding="utf-8")) if manifest.is_file() else {}
        npm.append({
            "name": installed.get("name", locator.rsplit("node_modules/", 1)[-1]),
            "version": package.get("version"),
            "declared_license": installed.get("license", package.get("license")),
            "notice_files": notices(directory) if directory.is_dir() else [],
            "installed": manifest.is_file(), "dev": package.get("dev", False),
            "optional": package.get("optional", False), "scope": "npm-lock-all-packages",
        })
    dependencies = {"cargo": rust, "npm": npm}
    gaps = []
    for ecosystem, entries in dependencies.items():
        for entry in entries:
            reasons = []
            if not entry["declared_license"]:
                reasons.append("missing-license-metadata")
            if not entry["notice_files"]:
                reasons.append("no-top-level-notice-located")
            if ecosystem == "npm" and not entry["installed"]:
                reasons.append("not-installed-on-this-platform")
            if reasons:
                gaps.append({"ecosystem": ecosystem, "name": entry["name"],
                             "version": entry["version"], "reasons": reasons})
    return {
        "schema_version": 1,
        "recorded_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "project_license": "MIT" if (root / "LICENSE").is_file() else None,
        "release_approved": False,
        "scope": "Local offline Windows x64 Cargo metadata and npm lock/top-level notices, including dev/optional dependencies. Cargo may list packages outside the filtered resolve graph. No SPDX legal interpretation or shipped-binary reachability is asserted.",
        "counts": {key: len(value) for key, value in dependencies.items()},
        "dependencies": dependencies, "review_gaps": gaps,
        "remaining_gates": [
            "Review selected licenses and actual required notices for shipped components",
            "Review embedded SQLite and Windows/WebView2 redistribution requirements separately",
            "Produce release builds and installer with all three matching executables",
            "Include offline WebView2 and verify a clean disconnected Windows installation",
            "Verify PATH, startup, update/uninstall and data preservation",
            "Review source/assets/secrets/publication allowlist and complete acceptance",
        ],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    options = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    result = collect(root)
    # Exclusive creation prevents replacing an earlier audit or user file.
    with options.output.open("x", encoding="utf-8", newline="\n") as output:
        json.dump(result, output, ensure_ascii=False, indent=2)
        output.write("\n")
    print(json.dumps({"counts": result["counts"], "review_gap_count": len(result["review_gaps"]),
                      "release_approved": False}))


if __name__ == "__main__":
    main()
