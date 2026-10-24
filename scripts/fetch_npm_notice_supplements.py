"""Collect pinned upstream notice candidates for installed npm notice gaps.

Review evidence only: no package installation, license clearance, or source rewrite.
"""
import argparse
import hashlib
import json
import re
import urllib.parse
import urllib.request
from pathlib import Path


SOURCES = {
    "@esbuild/win32-x64": ("evanw/esbuild", ["LICENSE.md"], None),
    "@rollup/rollup-win32-x64-gnu": ("rollup/rollup", ["LICENSE.md", "LICENSE-CORE.md"], None),
    "@rollup/rollup-win32-x64-msvc": ("rollup/rollup", ["LICENSE.md", "LICENSE-CORE.md"], None),
    "@tauri-apps/cli-win32-x64-msvc": ("tauri-apps/tauri", ["LICENSE-MIT", "LICENSE-APACHE-2.0"], "@tauri-apps/cli"),
    "saxes": ("lddubeau/saxes", ["LICENSE"], None),
}


def download(url):
    request = urllib.request.Request(url, headers={"User-Agent": "Swarm-notice-review"})
    with urllib.request.urlopen(request, timeout=20) as response:
        if response.geturl() != url:
            raise ValueError("Unexpected redirect")
        data = response.read(1024 * 1024 + 1)
    if not data or len(data) > 1024 * 1024:
        raise ValueError("Empty/oversized source")
    data.decode("utf-8", errors="strict")
    return data


def metadata(name, version):
    url = "https://registry.npmjs.org/" + urllib.parse.quote(name, safe="") + "/" + version
    raw = download(url)
    data = json.loads(raw)
    if data["name"] != name or data["version"] != version:
        raise ValueError("Registry identity mismatch")
    return data, {"url": url, "sha256": hashlib.sha256(raw).hexdigest()}


def collect(root, output):
    if output.exists():
        raise ValueError("Preserve existing review output")
    lock_path = root / "apps/desktop/package-lock.json"
    lock_bytes = lock_path.read_bytes()
    lock = json.loads(lock_bytes)
    records = []
    for name, (repo, filenames, wrapper) in SOURCES.items():
        manifest_path = root / "apps/desktop/node_modules" / name / "package.json"
        manifest_bytes = manifest_path.read_bytes()
        installed = json.loads(manifest_bytes)
        locked = lock["packages"]["node_modules/" + name]
        if installed["name"] != name or installed["version"] != locked["version"]:
            raise ValueError("Installed/locked version drift")
        data, proof = metadata(name, installed["version"])
        expected_repo = "git+https://github.com/" + repo + ".git"
        if data.get("repository", {}).get("url") != expected_repo:
            raise ValueError("Repository mismatch")
        proofs = [proof]
        provenance = "package_registry_gitHead"
        if wrapper:
            parent, parent_proof = metadata(wrapper, installed["version"])
            if parent.get("optionalDependencies", {}).get(name) != installed["version"]:
                raise ValueError("Exact wrapper/platform dependency required")
            if parent.get("repository", {}).get("url") != expected_repo:
                raise ValueError("Wrapper repository mismatch")
            proofs.append(parent_proof)
            data = parent
            provenance = "same_version_wrapper_gitHead_review_candidate"
        commit = data.get("gitHead", "")
        if not re.fullmatch(r"[0-9a-f]{40}", commit):
            raise ValueError("Immutable commit required")
        records.append({"name": name, "version": installed["version"],
                        "declared_license": installed.get("license"),
                        "manifest_sha256": hashlib.sha256(manifest_bytes).hexdigest(),
                        "repository": repo, "commit": commit, "provenance": provenance,
                        "registry_evidence": proofs, "filenames": filenames})
    output.mkdir(parents=True, exist_ok=False)
    for record in records:
        record["files"] = []
        for filename in record.pop("filenames"):
            url = f"https://raw.githubusercontent.com/{record['repository']}/{record['commit']}/{filename}"
            raw = download(url)
            if b"<html" in raw.lower():
                raise ValueError("Expected notice text, received HTML")
            destination = output / record["name"].replace("/", "__").replace("@", "") / filename
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(raw)
            record["files"].append({"url": url, "path": str(destination.resolve()),
                                    "bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest()})
    report = {"schema_version": 1, "release_approved": False,
              "scope": "Pinned upstream review candidates; applicability and shipped scope remain open",
              "lock_sha256": hashlib.sha256(lock_bytes).hexdigest(), "packages": records}
    (output / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    output = args.output.resolve()
    if not output.is_relative_to(root / "target/distribution"):
        parser.error("Choose a fresh review folder under target/distribution")
    report = collect(root, output)
    print(json.dumps({"packages": len(report["packages"]),
                      "files": sum(len(p["files"]) for p in report["packages"]),
                      "release_approved": False}))
