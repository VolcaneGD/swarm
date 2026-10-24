"""Inventory a conservative source-publication candidate; never copy or publish."""
import argparse
import hashlib
import json
import re
from pathlib import Path


ROOT_FILES = {"Cargo.toml", "Cargo.lock", "LICENSE", "icon.png", ".gitignore",
              "README.md", "README.en.md", "PROJECT_SPEC.md", "仕様書.md", "AGENT.md"}
SOURCE_ROOTS = {"crates", "apps", "scripts"}
THIRD_PARTY_FILES = {"THIRD_PARTY_NOTICES.txt", "SOURCE_AVAILABILITY.txt", "MPL-2.0.txt", "bundle-manifest.json",
                     "cssparser-0.37.0.crate", "cssparser-macros-0.7.1.crate", "dtoa-short-0.3.5.crate",
                     "option-ext-0.2.0.crate", "selectors-0.38.0.crate"}
SOURCE_SUFFIXES = {".rs", ".toml", ".lock", ".json", ".tsx", ".ts", ".js",
                   ".html", ".css", ".svg", ".png", ".ico", ".py", ".ps1", ".nsh", ".sql"}
SKIP_DIRS = {"target", "node_modules", "dist", "build", "gen", "__pycache__",
             ".git", ".codegraph", ".ai-memory", "evidence"}
PRIVATE_DOCS = {"HANDOVER.md", "Development_Log.md"}
BUILD_SUPPORT = {".agents/skills/swarm-cli/SKILL.md",
                 ".agents/skills/swarm-cli/agents/openai.yaml", ".cargo/config.toml",
                 "docs/cli-context.example.json"}
PUBLIC_TEST_FIXTURES = {"crates/swarm-core/tests/fixtures/google-test-private.pem"}


def classify(relative):
    """A selected path remains a review candidate, not a secret-free assertion."""
    parts = relative.parts
    if any(part in SKIP_DIRS for part in parts):
        return "generated-cache-or-evidence"
    if relative.name.startswith(".env") or relative.name.upper() == "RULES.MD":
        return "local-configuration"
    if relative.as_posix() in BUILD_SUPPORT:
        return None
    if relative.as_posix() in PUBLIC_TEST_FIXTURES:
        return None
    if parts[:2] == ("distribution", "third-party") and len(parts) == 3 and relative.name in THIRD_PARTY_FILES:
        return None
    if any(part.startswith(".") for part in parts) and relative.as_posix() != ".gitignore":
        return "hidden-or-agent-local"
    if len(parts) == 1 and relative.name in ROOT_FILES:
        return None
    if parts[0] == "docs" and len(parts) == 2 and relative.suffix == ".md":
        return "session-history" if relative.name in PRIVATE_DOCS else None
    if parts[0] in SOURCE_ROOTS and relative.suffix in SOURCE_SUFFIXES:
        return None
    return "outside-explicit-source-scope"


def inventory(root):
    root = root.resolve()
    if not all((root / name).is_file() for name in ("Cargo.toml", "Cargo.lock", "LICENSE")):
        raise ValueError("Source root requires Cargo.toml, Cargo.lock and LICENSE")
    selected, excluded = [], []

    def visit(directory):
        for path in sorted(directory.iterdir()):
            relative = path.relative_to(root)
            # Python 3.11 lacks Path.is_junction; reject every Windows reparse
            # point rather than following a junction or other redirection.
            if path.is_symlink() or getattr(path.lstat(), "st_file_attributes", 0) & 0x400:
                raise ValueError("Refuse symlink/junction: " + relative.as_posix())
            if path.is_dir():
                if path.name in SKIP_DIRS or (path.name.startswith(".") and
                    not any(name.startswith(relative.as_posix() + "/") for name in BUILD_SUPPORT)):
                    excluded.append({"path": relative.as_posix() + "/", "reason": "pruned-directory"})
                else:
                    visit(path)
            elif path.is_file():
                reason = classify(relative)
                if reason:
                    excluded.append({"path": relative.as_posix(), "reason": reason})
                else:
                    raw = path.read_bytes()
                    selected.append({"path": relative.as_posix(), "bytes": len(raw),
                                     "sha256": hashlib.sha256(raw).hexdigest()})
            else:
                raise ValueError("Non-regular source entry: " + relative.as_posix())

    visit(root)
    selected_paths = {p["path"] for p in selected}
    build_inputs = set()
    for entry in selected:
        path = root / entry["path"]
        if path.suffix == ".rs":
            for literal in re.findall(r'include_(?:str|bytes)!\(\s*"([^"]+)"', path.read_text(encoding="utf-8")):
                required = (path.parent / literal).resolve()
                if not required.is_relative_to(root):
                    raise ValueError("Compile input outside source root")
                build_inputs.add(required.relative_to(root).as_posix())
    config = root / "apps/desktop/src-tauri/tauri.distribution.conf.json"
    if config.is_file():
        for literal in json.loads(config.read_text(encoding="utf-8"))["bundle"]["resources"]:
            required = (config.parent / literal).resolve()
            if not required.is_relative_to(root):
                raise ValueError("Packaging input outside source root")
            relative = required.relative_to(root).as_posix()
            if not relative.startswith("target/"):
                build_inputs.add(relative)
    return {"schema_version": 1, "release_approved": False, "candidate_only": True,
            "scope": "Path allowlist inventory; no content, secret, license or publication approval",
            "files": selected, "excluded": excluded,
            "build_input_gaps": sorted(build_inputs - selected_paths),
            "remaining_gates": ["Review selected contents for secrets, private paths and personal data",
                                "Review specification/example fixtures and public documentation",
                                "Third-party source/notice obligations and asset provenance",
                                "Rebuildability of source package including generated inputs",
                                "Explicit owner publication instruction and formal acceptance"]}


def write_review(root, output):
    root = root.resolve()
    output = output.resolve()
    if not output.is_relative_to(root / "target/distribution") or output.suffix != ".json":
        raise ValueError("Review output must be JSON under target/distribution")
    if output.exists():
        raise ValueError("Preserve existing review output")
    report = inventory(root)
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("x", encoding="utf-8", newline="\n") as stream:
        json.dump(report, stream, ensure_ascii=False, indent=2)
        stream.write("\n")
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    result = write_review(Path(__file__).resolve().parents[1], arguments.output)
    print(json.dumps({"files": len(result["files"]), "excluded": len(result["excluded"]),
                      "release_approved": False}))
