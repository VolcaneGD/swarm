"""Fetch explicitly reviewed upstream license files pinned to installed crate commits.

These are review supplements, not automatic replacements for missing notices.
No credentials, packages, installers or production settings are changed.
"""
import argparse
import json
import re
import subprocess
import urllib.request
from pathlib import Path

from collect_distribution_notices import digest


# Filenames confirmed in these upstream repositories. Never fetch a moving ref.
SOURCES = {
    "alloc-stdlib": ("https://github.com/dropbox/rust-alloc-no-stdlib", ["LICENSE"]),
    "defmt-parser": ("https://github.com/knurling-rs/defmt", ["LICENSE-MIT", "LICENSE-APACHE"]),
    "tauri-plugin": ("https://github.com/tauri-apps/tauri", ["LICENSE-MIT", "LICENSE-APACHE-2.0"]),
    "webview2-com": ("https://github.com/wravery/webview2-rs", ["LICENSE"]),
    "webview2-com-macros": ("https://github.com/wravery/webview2-rs", ["LICENSE"]),
    "webview2-com-sys": ("https://github.com/wravery/webview2-rs", ["LICENSE"]),
}


def pinned_sources(package):
    repository, filenames = SOURCES[package["name"]]
    if package["repository"] != repository:
        raise ValueError("Repository differs from reviewed source")
    vcs_path = Path(package["manifest_path"]).parent / ".cargo_vcs_info.json"
    vcs_bytes = vcs_path.read_bytes()
    vcs = json.loads(vcs_bytes)
    commit = vcs["git"]["sha1"]
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError("Expected immutable commit ID")
    slug = repository.removeprefix("https://github.com/")
    return {"name": package["name"], "version": package["version"],
            "declared_license": package.get("license"), "repository": repository,
            "commit": commit, "crate_vcs_sha256": digest(vcs_bytes),
            "path_in_vcs": vcs.get("path_in_vcs"),
            "files": [{"filename": f, "url": f"https://raw.githubusercontent.com/{slug}/{commit}/{f}"}
                      for f in filenames]}


def fetch(root, output):
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--format-version", "1", "--locked", "--offline",
         "--filter-platform", "x86_64-pc-windows-msvc"], cwd=root, text=True, encoding="utf-8"))
    packages = [pinned_sources(p) for p in metadata["packages"] if p["name"] in SOURCES]
    if len(packages) != len(SOURCES):
        raise ValueError("Reviewed crate set changed")
    output.mkdir(parents=True, exist_ok=False)
    for package in packages:
        for record in package["files"]:
            request = urllib.request.Request(record["url"], headers={"User-Agent": "Swarm-notice-review"})
            with urllib.request.urlopen(request, timeout=20) as response:
                if response.geturl() != record["url"]:
                    raise ValueError("Unexpected source redirect")
                content = response.read(1024 * 1024 + 1)
            if not content or len(content) > 1024 * 1024:
                raise ValueError("Empty/oversized source text")
            content.decode("utf-8", errors="strict")
            if b"<html" in content.lower():
                raise ValueError("Expected license text, received HTML")
            destination = output / package["name"] / record["filename"]
            destination.parent.mkdir(exist_ok=True)
            with destination.open("xb") as file:
                file.write(content)
            record.update({"bytes": len(content), "sha256": digest(content),
                           "local_path": destination.relative_to(output).as_posix()})
    report = {"schema_version": 1, "release_approved": False,
              "scope": "Exact upstream license bytes from the commits declared by locally installed crates. Repository-wide applicability, copyright/NOTICE completeness and SPDX branch decisions still require review. selectors has no relevant upstream license file located; no substitute text is assigned.",
              "cargo_lock_sha256": digest((root / "Cargo.lock").read_bytes()), "packages": packages}
    with (output / "supplement-review.json").open("x", encoding="utf-8", newline="\n") as file:
        json.dump(report, file, indent=2)
        file.write("\n")
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    output = args.output.resolve()
    if not output.is_relative_to(root / "target/distribution"):
        parser.error("Choose a fresh review folder under target/distribution")
    report = fetch(root, output)
    print(json.dumps({"packages": len(report["packages"]), "files": sum(len(p["files"]) for p in report["packages"]),
                      "release_approved": False}))
