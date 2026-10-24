"""Prepare exact locked selectors source and steward MPL text for local review.

No compliance declaration, source publication or replacement package notice.
"""
import argparse
import io
import json
import subprocess
import tarfile
import tomllib
import urllib.request
from pathlib import Path

from assemble_notice_review import contained
from collect_distribution_notices import digest


def verify_source(archive, package, source, checksum):
    if archive.stat().st_size > 16 * 1024 * 1024:
        raise ValueError("Oversized cached crate")
    raw = archive.read_bytes()
    if digest(raw) != checksum:
        raise ValueError("Cached crate differs from locked checksum")
    records = []
    with tarfile.open(fileobj=io.BytesIO(raw), mode="r:gz") as tar:
        for member in tar.getmembers():
            prefix = f"{package['name']}-{package['version']}/"
            if not member.isfile() or not member.name.startswith(prefix) or member.size > 4 * 1024 * 1024:
                raise ValueError("Invalid crate member")
            relative = member.name.removeprefix(prefix)
            path = contained(source, source / relative)
            with tar.extractfile(member) as file:
                data = file.read(4 * 1024 * 1024 + 1)
            if path.read_bytes() != data:
                raise ValueError("Installed source differs from locked archive")
            records.append({"path": relative, "bytes": len(data), "sha256": digest(data),
                            "mpl_header": b"Mozilla Public License" in data[:1500]})
    return raw, records


def prepare(root, output):
    output = contained(root / "target/distribution", output)
    if output.exists():
        raise ValueError("Preserve existing source review")
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--format-version", "1", "--locked", "--offline",
         "--filter-platform", "x86_64-pc-windows-msvc"], cwd=root, text=True, encoding="utf-8"))
    package = next(p for p in metadata["packages"] if p["name"] == "selectors")
    if package["version"] != "0.38.0" or package["license"] != "MPL-2.0" or package["repository"] != "https://github.com/servo/stylo":
        raise ValueError("Reviewed selectors identity changed")
    source = Path(package["manifest_path"]).parent
    lock_raw = (root / "Cargo.lock").read_bytes()
    locked = next(p for p in tomllib.loads(lock_raw.decode("utf-8"))["package"]
                  if p["name"] == package["name"] and p["version"] == package["version"])
    archive = source.parents[2] / "cache" / source.parent.name / (source.name + ".crate")
    raw, records = verify_source(archive, package, source, locked["checksum"])
    url = "https://www.mozilla.org/media/MPL/2.0/index.txt"
    with urllib.request.urlopen(urllib.request.Request(url, headers={"User-Agent": "Swarm-notice-review"}), timeout=20) as response:
        if response.geturl() != url:
            raise ValueError("Unexpected steward redirect")
        license_raw = response.read(1024 * 1024 + 1)
    if not license_raw.startswith(b"Mozilla Public License Version 2.0") or len(license_raw) > 1024 * 1024:
        raise ValueError("Unexpected steward license text")
    license_raw.decode("utf-8", errors="strict")
    result = {"schema_version": 1, "release_approved": False, "name": package["name"], "version": package["version"],
              "declared_license": package["license"], "cargo_lock_sha256": digest(lock_raw),
              "repository": package["repository"], "vcs": json.loads((source / ".cargo_vcs_info.json").read_bytes()),
              "source_archive": {"path": source.name + ".crate", "bytes": len(raw), "sha256": digest(raw)},
              "steward_license": {"path": "MPL-2.0.txt", "url": url, "bytes": len(license_raw), "sha256": digest(license_raw)},
              "verified_archive_files": records,
              "scope": "All archive files match installed source; canonical steward license is separate from a package notice. No extraction or public upload.",
              "remaining": ["Actual shipped/generated-code applicability", "Recipient source-location notice and final bundle integration",
                            "Whole-package attribution/license review and explicit publication decision"]}
    output.mkdir(parents=True, exist_ok=False)
    for name, content in [(source.name + ".crate", raw), ("MPL-2.0.txt", license_raw)]:
        with (output / name).open("xb") as file:
            file.write(content)
    with (output / "source-review.json").open("x", encoding="utf-8", newline="\n") as file:
        json.dump(result, file, ensure_ascii=False, indent=2)
        file.write("\n")
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    result = prepare(Path(__file__).resolve().parents[1], args.output)
    print(json.dumps({"files_verified": len(result["verified_archive_files"]),
                      "source_sha256": result["source_archive"]["sha256"], "release_approved": False}))
