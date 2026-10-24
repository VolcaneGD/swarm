"""Read-only acceptance bookkeeping; never executes product tests or accepts ACs.

Only snapshot writes a new, explicitly named source-hash file (exclusive create).
No subprocess, network, app, credential, fixture or OS operation is performed.
"""
import argparse
from collections import Counter
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path, PureWindowsPath
import re
import sys
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]
DOCS = ("docs/ROADMAP.md", "PROJECT_SPEC.md", "docs/HANDOVER.md", "docs/REMAINING_WORK.md")
SKIP = {"target", "node_modules", "dist", "build", ".git", ".serena", "__pycache__"}
TEXT = {".md", ".rs", ".toml", ".sql", ".ps1", ".py", ".ts", ".tsx", ".css", ".html", ".json"}
PRODUCT = TEXT - {".md", ".ps1", ".py"}
ROOT_PRODUCT = ("Cargo.toml", "Cargo.lock")
ROW = re.compile(r"^\s*- \[([ x])\] (.+)$", re.M)
MARKER = re.compile(r"<!-- acceptance-status: checked=(\d+) open=(\d+) total=(\d+) -->")
GROUPS = {"human", "safety", "blocked", "held", "publication", "aggregate", "decision", "workflow"}


def read(path):
    value = path.read_text(encoding="utf-8")
    if "\ufffd" in value:
        raise ValueError(f"replacement character: {path.name}")
    return value


def safe_path(root, name):
    """Reject absolute, drive, traversal and escaping symlink paths before reading."""
    if not isinstance(name, str) or not name or PureWindowsPath(name).drive:
        raise ValueError("invalid relative path")
    relative = Path(name.replace("\\", "/"))
    if relative.is_absolute() or ".." in relative.parts:
        raise ValueError("unsafe relative path")
    path = root.joinpath(relative).resolve()
    if not path.is_relative_to(root.resolve()):
        raise ValueError("path escapes selected root")
    return path


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def paths(root, folders, suffixes):
    for folder in folders:
        base = root / folder
        if not base.is_dir():
            continue
        for directory, dirs, files in os.walk(base, followlinks=False):
            dirs[:] = sorted(d for d in dirs if d not in SKIP and not Path(directory, d).is_symlink())
            for name in sorted(files):
                path = Path(directory, name)
                if path.suffix in suffixes or name == "Cargo.lock":
                    yield safe_path(root, path.relative_to(root).as_posix())


def source_hashes(root):
    selected = list(paths(root, ("crates", "apps"), PRODUCT))
    selected += [root / name for name in ROOT_PRODUCT if (root / name).is_file()]
    if not selected:
        raise ValueError("empty product source inventory")
    return {p.relative_to(root).as_posix(): digest(p) for p in sorted(selected)}


def status(root):
    roadmap = read(root / DOCS[0])
    rows = [{"checked": mark == "x", "text": text, "line": roadmap[:m.start()].count("\n") + 1}
            for m in ROW.finditer(roadmap) for mark, text in [m.groups()]]
    counts = {"checked": sum(r["checked"] for r in rows), "open": sum(not r["checked"] for r in rows), "total": len(rows)}
    plan = read(root / "docs/REMAINING_WORK.md")
    match = re.search(r"<!-- acceptance-queue:start -->\n(.*?)<!-- acceptance-queue:end -->", plan, re.S)
    if not match:
        raise ValueError("acceptance queue missing")
    entries, seen, assigned = [], set(), set()
    for line in match[1].splitlines():
        if not line.startswith("| R") and not line.startswith("| W"):
            continue
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) != 5:
            raise ValueError("invalid queue row")
        item_id, locator, group, batch, prerequisite = cells
        if item_id in seen or group not in GROUPS or not all(cells):
            raise ValueError(f"invalid queue identity/group: {item_id}")
        found = [r for r in rows if r["text"].startswith(locator)]
        if len(found) != 1 or found[0]["line"] in assigned:
            raise ValueError(f"queue locator not unique: {item_id}")
        seen.add(item_id)
        assigned.add(found[0]["line"])
        entries.append({"id": item_id, "group": group, "batch": batch,
                        "prerequisite": prerequisite, **found[0]})
    if {x["id"] for x in entries if x["id"].startswith("R")} != {f"R{i:02}" for i in range(1, 26)}:
        raise ValueError("original 25 residual identities must be preserved")
    extra = [r["line"] for r in rows if not r["checked"] and r["line"] not in assigned]
    residuals = [x for x in entries if x["id"].startswith("R")]
    return {"roadmap": counts, "original_residuals": {"closed": sum(x["checked"] for x in residuals),
            "open": sum(not x["checked"] for x in residuals), "total": 25},
            "open_by_primary_group": dict(Counter(x["group"] for x in entries if not x["checked"])),
            "unclassified_open_lines": extra, "queue": entries,
            "acceptance": "NOT_ASSESSED", "publication_authorized": False}


def local_links(root, name, text):
    text = re.sub(r"```.*?```|`[^`\n]*`", "", text, flags=re.S)
    for target in re.findall(r"\]\(([^)]+)\)", text):
        parsed = urlsplit(target.strip("<>"))
        if parsed.scheme or parsed.netloc or not parsed.path:
            continue
        path = (root / name).parent / unquote(parsed.path)
        if not path.resolve().is_relative_to(root.resolve()) or not path.exists():
            raise ValueError(f"missing/outside local link in {name}: {target}")


def check(root):
    result = status(root)
    if result["unclassified_open_lines"]:
        raise ValueError(f"unclassified open ROADMAP lines: {result['unclassified_open_lines']}")
    counts = result["roadmap"]
    expected = (counts["checked"], counts["open"], counts["total"])
    for name in DOCS:
        text = read(root / name)
        marker = MARKER.search(text[:1800])
        if not marker or tuple(map(int, marker.groups())) != expected:
            raise ValueError(f"stale/missing current count marker: {name}; expected {expected}")
        local_links(root, name, text[:text.index("<!-- acceptance-current:end -->")]
                    if "<!-- acceptance-current:end -->" in text else text[:1800])
    selected = list(paths(root, ("crates", "apps", "docs", "scripts"), TEXT))
    selected += [p for p in root.iterdir() if p.is_file() and (p.suffix in TEXT or p.name == "Cargo.lock")]
    for path in selected:
        read(path)
    # Only maintained queue/procedure sections are whitespace/link checked; history is not rewritten.
    plan = read(root / "docs/REMAINING_WORK.md").split("<!-- acceptance-queue:end -->")[0]
    if any(line != line.rstrip() for line in plan.splitlines()):
        raise ValueError("trailing whitespace in current plan")
    local_links(root, "docs/REMAINING_WORK.md", plan)
    result["utf8_files"] = len(selected)
    result["bookkeeping_check"] = "PASS"
    return result


def verify_source(root, baseline):
    saved = json.loads(read(baseline))
    if saved.get("schema_version") != 1 or saved.get("root") != str(root.resolve()) or not saved.get("files"):
        raise ValueError("wrong/empty baseline or workspace identity")
    for name, sha in saved["files"].items():
        safe_path(root, name)
        if not re.fullmatch(r"[0-9a-f]{64}", sha):
            raise ValueError("invalid baseline digest")
    current = source_hashes(root)
    changed = sorted(name for name in set(saved["files"]) | set(current) if saved["files"].get(name) != current.get(name))
    if changed:
        raise ValueError("product source changed: " + ", ".join(changed))
    return len(current)


def verify_candidate(root, candidate):
    candidate = candidate.resolve()
    if not candidate.is_relative_to((root / "target/distribution").resolve()):
        raise ValueError("candidate must be inside selected workspace target/distribution")
    report = json.loads(read(candidate / "package-report.json"))
    if report.get("release_approved") is not False or report.get("candidate_only") is not True:
        raise ValueError("expected unpublished candidate report")
    if not report.get("files") or not report.get("installers"):
        raise ValueError("empty candidate/installer manifest")
    seen = set()
    for directory, items in ((candidate, report["files"]), (root, report["installers"])):
        for item in items:
            path = safe_path(directory, item["path"])
            if not path.is_relative_to(candidate) or path in seen:
                raise ValueError("candidate manifest duplicate/outside candidate")
            seen.add(path)
            if digest(path) != item["sha256"]:
                raise ValueError("candidate hash mismatch: " + item["path"])
    return len(seen)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("status", "check", "snapshot"))
    parser.add_argument("--json", action="store_true", help="machine-readable stdout; no file write")
    parser.add_argument("--output", type=Path, help="snapshot only: new source-hash file; never overwrite")
    parser.add_argument("--baseline", type=Path, help="check only: compare present product sources to prior snapshot")
    parser.add_argument("--candidate", type=Path, help="check only: verify explicit existing candidate; never rebuild")
    args = parser.parse_args(argv)
    try:
        if args.command == "snapshot":
            if not args.output or args.baseline or args.candidate:
                raise ValueError("snapshot requires --output only")
            result = {"schema_version": 1, "root": str(ROOT.resolve()),
                      "recorded_at": datetime.now(timezone.utc).isoformat(), "files": source_hashes(ROOT)}
            with args.output.open("x", encoding="utf-8") as stream:
                json.dump(result, stream, indent=2)
                stream.write("\n")
            result = {"snapshot": str(args.output), "source_files": len(result["files"]), "acceptance": "NOT_ASSESSED"}
        else:
            if args.output or (args.command == "status" and (args.baseline or args.candidate)):
                raise ValueError("baseline/candidate are check-only; output is snapshot-only")
            result = check(ROOT) if args.command == "check" else status(ROOT)
            if args.baseline:
                result["unchanged_product_files"] = verify_source(ROOT, args.baseline)
            if args.candidate:
                result["candidate_hashes_verified"] = verify_candidate(ROOT, args.candidate)
        if args.json:
            print(json.dumps(result, ensure_ascii=False, indent=2))
        elif "roadmap" in result:
            print("ROADMAP:", result["roadmap"], "original residuals:", result["original_residuals"])
            print("Open by primary group:", result["open_by_primary_group"])
            for entry in result["queue"]:
                if not entry["checked"]:
                    print(f"{entry['id']} [{entry['group']}/{entry['batch']}] {entry['prerequisite']}")
            print({k: v for k, v in result.items() if k in ("bookkeeping_check", "utf8_files", "unchanged_product_files", "candidate_hashes_verified")})
            print("Bookkeeping only; no AC or publication approval.")
        else:
            print(json.dumps(result, ensure_ascii=False))
        return 0
    except (ValueError, OSError, KeyError, TypeError) as error:
        print(f"acceptance_status: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    # Keep Japanese CLI output stable through Windows terminals and captured pipes.
    for stream in (sys.stdout, sys.stderr):
        if hasattr(stream, "reconfigure"):
            stream.reconfigure(encoding="utf-8")
    sys.exit(main())
