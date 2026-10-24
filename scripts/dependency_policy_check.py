"""Create a local locked-dependency inventory and bounded policy report.

This checks Cargo lock/manifest coherence through offline Cargo metadata,
checks direct npm manifest ranges against package-lock versions with the
repository's installed semver package, and records declared license metadata
and top-level notice locators. It does not determine shipped reachability,
interpret SPDX compatibility, or approve a release.
"""

import argparse
import hashlib
import json
import re
import subprocess
import tomllib
from pathlib import Path

from distribution_audit import collect


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def npm_range_check(root):
    desktop = root / "apps" / "desktop"
    node_script = r"""
const fs = require('node:fs');
const path = require('node:path');
const root = process.argv[1];
const desktop = path.join(root, 'apps', 'desktop');
const manifest = JSON.parse(fs.readFileSync(path.join(desktop, 'package.json'), 'utf8'));
const lock = JSON.parse(fs.readFileSync(path.join(desktop, 'package-lock.json'), 'utf8'));
const semver = require(path.join(desktop, 'node_modules', 'semver'));
const lockedRoot = lock.packages && lock.packages[''];
const checks = [];
const errors = [];
if (!lockedRoot) errors.push('package-lock has no root package entry');
if (lock.lockfileVersion !== 3) errors.push(`expected package-lock v3, got ${lock.lockfileVersion}`);
for (const section of ['dependencies', 'devDependencies', 'optionalDependencies']) {
  const declared = manifest[section] || {};
  const locked = (lockedRoot && lockedRoot[section]) || {};
  if (JSON.stringify(Object.fromEntries(Object.entries(declared).sort())) !==
      JSON.stringify(Object.fromEntries(Object.entries(locked).sort()))) {
    errors.push(`${section} declarations differ between package.json and package-lock root`);
  }
  for (const [name, range] of Object.entries(declared)) {
    const item = lock.packages[`node_modules/${name}`];
    const version = item && item.version;
    const satisfied = typeof version === 'string' && semver.satisfies(version, range);
    checks.push({section, name, declared_range: range, locked_version: version || null,
                 range_satisfied: satisfied});
    if (!satisfied) errors.push(`${name}@${version || 'missing'} does not satisfy ${range}`);
  }
}
process.stdout.write(JSON.stringify({lockfile_version: lock.lockfileVersion,
  direct_dependency_count: checks.length, checks, errors}));
"""
    raw = subprocess.check_output(
        ["node", "-e", node_script, str(root)], cwd=desktop,
        text=True, encoding="utf-8",
    )
    return json.loads(raw)


def cargo_rust_version_check(root):
    manifest = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
    minimum = manifest["workspace"]["package"]["rust-version"]
    minimum_parts = tuple(int(part) for part in minimum.split("."))
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--format-version", "1", "--locked", "--offline",
         "--filter-platform", "x86_64-pc-windows-msvc"],
        cwd=root, text=True, encoding="utf-8",
    ))
    unresolved = []
    newer = []
    declared = 0
    for package in metadata["packages"]:
        version = package.get("rust_version")
        if not version:
            unresolved.append(package["name"])
            continue
        match = re.fullmatch(r"(\d+)\.(\d+)(?:\.(\d+))?", version)
        if not match:
            unresolved.append(package["name"])
            continue
        declared += 1
        parts = tuple(int(part) for part in match.groups(default="0"))
        padded_minimum = minimum_parts + (0,) * (3 - len(minimum_parts))
        if parts > padded_minimum:
            newer.append({"name": package["name"], "version": package["version"],
                          "declared_rust_version": version})
    return {
        "workspace_rust_version": minimum,
        "metadata_packages_checked": len(metadata["packages"]),
        "packages_with_parseable_rust_version": declared,
        "packages_without_parseable_rust_version": len(unresolved),
        "packages_requiring_newer_rust": newer,
        "version_check_scope": "Declared package rust-version metadata only; missing declarations do not establish compatibility.",
    }


def build_report(root):
    inventory = collect(root)
    npm = npm_range_check(root)
    cargo_rust = cargo_rust_version_check(root)
    cargo_lock = root / "Cargo.lock"
    npm_lock = root / "apps" / "desktop" / "package-lock.json"
    license_counts = {}
    for ecosystem, entries in inventory["dependencies"].items():
        license_counts[ecosystem] = {
            "packages": len(entries),
            "declared_license_present": sum(bool(item["declared_license"]) for item in entries),
            "top_level_notice_located": sum(bool(item["notice_files"]) for item in entries),
            "no_top_level_notice_located": sum(not item["notice_files"] for item in entries),
        }
    report = {
        "schema_version": 1,
        "project_license": inventory["project_license"],
        "release_approved": False,
        "scope": (
            "Offline Windows x64 Cargo metadata with --locked and --offline, plus all npm lock entries. "
            "Direct npm semver ranges are compared with package-lock versions. License fields are "
            "publisher metadata and located top-level notice filenames; this is not legal review, "
            "SPDX compatibility analysis, shipped-binary reachability, or publication approval."
        ),
        "lockfiles": {
            "cargo_lock_sha256": sha256(cargo_lock),
            "npm_lock_sha256": sha256(npm_lock),
            "cargo_metadata": {
                "command": "cargo metadata --format-version 1 --locked --offline --filter-platform x86_64-pc-windows-msvc",
                "completed": True,
                "package_count_in_inventory": len(inventory["dependencies"]["cargo"]),
                "declared_rust_version_check": cargo_rust,
            },
            "npm_direct_ranges": npm,
        },
        "license_inventory": {
            "counts": license_counts,
            "entries": inventory["dependencies"],
            "review_gaps": inventory["review_gaps"],
        },
        "bounded_policy_checks": {
            "cargo_manifest_lock_coherence": "pass: cargo metadata completed with --locked --offline",
            "declared_dependency_rust_version": "pass" if not cargo_rust["packages_requiring_newer_rust"] else "fail",
            "npm_manifest_lock_direct_ranges": "pass" if not npm["errors"] else "fail",
            "all_license_metadata_declared": all(
                values["declared_license_present"] == values["packages"]
                for values in license_counts.values()
            ),
            "required_notice_text_review": "open",
            "shipped_component_reachability_review": "open",
            "spdx_compatibility_and_legal_review": "open",
            "sqlite_webview2_and_asset_rights_review": "open",
            "publication_decision": "held until user says 公開してください",
        },
        "release_gates": inventory["remaining_gates"],
    }
    if npm["errors"]:
        report["bounded_policy_checks"]["npm_manifest_lock_errors"] = npm["errors"]
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    options = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    report = build_report(root)
    with options.output.open("x", encoding="utf-8", newline="\n") as output:
        json.dump(report, output, ensure_ascii=False, indent=2)
        output.write("\n")
    print(json.dumps({
        "output": str(options.output.resolve()),
        "counts": {key: value["packages"] for key, value in report["license_inventory"]["counts"].items()},
        "npm_direct_ranges": report["lockfiles"]["npm_direct_ranges"]["direct_dependency_count"],
        "npm_policy": report["bounded_policy_checks"]["npm_manifest_lock_direct_ranges"],
        "cargo_declared_rust_version_policy": report["bounded_policy_checks"]["declared_dependency_rust_version"],
        "license_metadata_complete": report["bounded_policy_checks"]["all_license_metadata_declared"],
        "release_approved": False,
    }))


if __name__ == "__main__":
    main()
