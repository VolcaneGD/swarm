"""Stage and inspect an unpublished Windows candidate, without installing it."""
import argparse
import datetime
import hashlib
import json
import re
import shutil
import struct
import subprocess
from pathlib import Path
from urllib.parse import unquote, urlsplit


def document_link_gaps(entries):
    """Check local Markdown links against the candidate's mapped destinations."""
    gaps = []
    for destination, source in entries.items():
        if source.suffix.lower() != ".md":
            continue
        text = source.read_text(encoding="utf-8")
        # Code examples are not Markdown navigation. Keep inline code out too.
        text = re.sub(r"```[^\n]*\n.*?```|~~~[^\n]*\n.*?~~~", "", text, flags=re.S)
        text = re.sub(r"`[^`\n]*`", "", text)
        targets = re.findall(r"!?\[[^\]\n]*\]\((<[^>\n]+>|[^\s)]+)(?:\s+[^)]*)?\)", text)
        targets += re.findall(r"^\s*\[[^\]\n]+\]:\s*(<[^>\n]+>|\S+)", text, flags=re.M)
        for target in targets:
            parsed = urlsplit(target.strip("<>"))
            if parsed.scheme or parsed.netloc or not parsed.path:
                continue
            path = unquote(parsed.path).replace("\\", "/")
            parts = [] if path.startswith("/") else list(Path(destination).parent.parts)
            outside = False
            for part in path.split("/"):
                if part in ("", "."):
                    continue
                if part == "..":
                    if not parts:
                        outside = True
                        break
                    parts.pop()
                else:
                    parts.append(part)
            resolved = "/".join(parts)
            if outside or resolved not in entries:
                gaps.append({"document": destination, "link": target,
                             "target": resolved, "outside_package": outside})
    return gaps


def file_hash(path):
    with path.open("rb") as file:
        return hashlib.file_digest(file, "sha256").hexdigest()


def pe_info(path):
    data = path.read_bytes()
    if data[:2] != b"MZ":
        raise ValueError(f"Not a PE executable: {path.name}")
    pe = struct.unpack_from("<I", data, 60)[0]
    if data[pe:pe + 4] != b"PE\0\0":
        raise ValueError("Invalid PE signature")
    machine, sections = struct.unpack_from("<HH", data, pe + 4)
    optional_size = struct.unpack_from("<H", data, pe + 20)[0]
    optional = pe + 24
    magic = struct.unpack_from("<H", data, optional)[0]
    if magic not in (0x10b, 0x20b):
        raise ValueError("Unsupported PE header")
    directory = optional + (112 if magic == 0x20b else 96)
    table = optional + optional_size

    def offset(rva):
        for index in range(sections):
            section = table + 40 * index
            length, address, raw_size, raw = struct.unpack_from("<IIII", data, section + 8)
            if address <= rva < address + max(length, raw_size):
                return raw + rva - address
        raise ValueError("PE RVA outside sections")

    imports = []
    import_rva = struct.unpack_from("<I", data, directory + 8)[0]
    if import_rva:
        descriptor = offset(import_rva)
        for _ in range(512):
            fields = struct.unpack_from("<IIIII", data, descriptor)
            if not any(fields):
                break
            start = offset(fields[3])
            imports.append(data[start:data.index(b"\0", start)].decode("ascii"))
            descriptor += 20
        else:
            raise ValueError("Unterminated PE import directory")
    types = []
    resource_rva = struct.unpack_from("<I", data, directory + 16)[0]
    if resource_rva:
        resource = offset(resource_rva)
        named, ids = struct.unpack_from("<HH", data, resource + 12)
        for index in range(named + ids):
            key = struct.unpack_from("<I", data, resource + 16 + index * 8)[0]
            if not key & 0x80000000:
                types.append(key)
    return {"machine": hex(machine), "subsystem": struct.unpack_from("<H", data, optional + 68)[0],
            "imported_dlls": imports, "resource_type_ids": types}


def mapped_entries(root):
    config_root = root / "apps/desktop/src-tauri"
    config = json.loads((config_root / "tauri.distribution.conf.json").read_text(encoding="utf-8"))
    entries = {"swarm-desktop.exe": root / "target/release/swarm-desktop.exe"}
    for source, destination in config["bundle"]["resources"].items():
        relative = Path(destination)
        if relative.is_absolute() or ".." in relative.parts or destination in entries:
            raise ValueError("Unsafe or duplicate destination")
        entries[destination] = (config_root / source).resolve()
    if not {"swarm-desktop.exe", "swarm.exe", "swarm-runtime.exe", "LICENSE"}.issubset(entries):
        raise ValueError("Candidate requires all three executables and LICENSE")
    return entries


def stage(root, output):
    entries = mapped_entries(root)
    for source in entries.values():
        if not source.is_relative_to(root) or not source.is_file():
            raise ValueError("Missing or outside-workspace source")
    gaps = document_link_gaps(entries)
    if gaps:
        raise ValueError("Unbundled local document links: " + json.dumps(gaps, ensure_ascii=False))
    output.mkdir(parents=True, exist_ok=False)
    records = []
    for destination, source in entries.items():
        path = output / destination
        path.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, path)
        record = {"path": destination, "bytes": path.stat().st_size,
                  "sha256": file_hash(path)}
        if record["sha256"] != file_hash(source):
            raise ValueError("Source changed during staging")
        if path.suffix == ".exe":
            record["pe"] = pe_info(path)
            expected = 3 if path.name == "swarm.exe" else 2
            if record["pe"]["machine"] != "0x8664" or record["pe"]["subsystem"] != expected:
                raise ValueError(f"Incorrect architecture/subsystem: {path.name}")
            # UCRT/APISet imports are Windows 11 OS components, unlike the
            # separately redistributed Visual C++ runtime DLLs below.
            if any(name.lower().startswith(("vcruntime", "msvcp", "msvcr1"))
                   for name in record["pe"]["imported_dlls"]):
                raise ValueError(f"Unexpected redistributable VC dependency: {path.name}")
            if path.name == "swarm-desktop.exe" and 14 not in record["pe"]["resource_type_ids"]:
                raise ValueError("Missing GUI icon resource")
        records.append(record)
    # These two CLI paths do not open the runtime, authenticate or create data.
    for option in ["--version", "--help"]:
        subprocess.run([str(output / "swarm.exe"), option], check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10)
    installers = []
    for path in sorted((root / "target/release/bundle/nsis").glob("*-setup.exe")):
        # A report must retain the reviewed installer after another build.
        # Use a byte copy, not a hard link to the mutable build output.
        expected_hash = file_hash(path)
        snapshot = output / "installers" / path.name
        snapshot.parent.mkdir(parents=True, exist_ok=True)
        if snapshot.exists():
            raise ValueError("Installer snapshot destination already exists")
        shutil.copyfile(path, snapshot)
        if file_hash(snapshot) != expected_hash or file_hash(path) != expected_hash:
            raise ValueError("Installer changed during staging")
        installers.append({"path": snapshot.relative_to(root).as_posix(),
                           "source_path": path.relative_to(root).as_posix(),
                           "bytes": snapshot.stat().st_size, "sha256": expected_hash,
                           "pe": pe_info(snapshot)})
    report = {"schema_version": 1, "recorded_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
              "candidate_only": True, "release_approved": False, "files": records, "installers": installers,
              "cli_smoke": ["--version", "--help"],
              "remaining_gates": ["Required third-party notices and complete documentation bundle",
                                  "Clean offline Windows installation and native acceptance",
                                  "User PATH/startup/update/uninstall/data preservation",
                                  "Signing/publication decision and all formal AC acceptance"]}
    with (output / "package-report.json").open("x", encoding="utf-8", newline="\n") as file:
        json.dump(report, file, indent=2)
        file.write("\n")
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--check-documents", action="store_true", help="Read-only document mapping preflight; refuses missing local links")
    args = parser.parse_args()
    task_root = Path(__file__).resolve().parent.parent
    if args.check_documents:
        if args.output:
            parser.error("--check-documents does not create an output directory")
        entries = mapped_entries(task_root)
        documents = {name: path for name, path in entries.items() if path.suffix.lower() == ".md"}
        if any(not path.is_relative_to(task_root) or not path.is_file() for path in documents.values()):
            parser.error("Missing or outside-workspace document source")
        gaps = document_link_gaps(entries)
        print(json.dumps({"release_approved": False, "documents": len(documents), "missing_links": gaps}, ensure_ascii=False, indent=2))
        raise SystemExit(1 if gaps else 0)
    if not args.output:
        parser.error("--output is required for staging")
    candidate_output = args.output.resolve()
    if not candidate_output.is_relative_to(task_root / "target/distribution"):
        parser.error("Output must be a fresh folder under target/distribution")
    result = stage(task_root, candidate_output)
    print(json.dumps({"files": len(result["files"]), "installers": len(result["installers"]),
                      "release_approved": False}))
