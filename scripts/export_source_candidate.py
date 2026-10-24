"""Export a local review-only source tree; preserve canonical files and publication hold."""
import argparse
import hashlib
import json
import re
from pathlib import Path
from source_package_review import inventory
from package_windows_candidate import document_link_gaps


def export(root, output):
    root = root.resolve()
    if output.is_symlink():
        raise ValueError("Linked source export")
    output = output.resolve()
    if not output.is_relative_to(root / "target/distribution") or output.exists():
        raise ValueError("Use a fresh source candidate under target/distribution")
    review = inventory(root)
    if review["build_input_gaps"]:
        raise ValueError("Source build inputs are missing")
    prepared = {}
    transformations = []
    for record in review["files"]:
        relative = record["path"]
        raw = (root / relative).read_bytes()
        if hashlib.sha256(raw).hexdigest() != record["sha256"]:
            raise ValueError("Source changed during review")
        if relative == "AGENT.md":
            text = raw.decode("utf-8")
            lines = text.splitlines(keepends=True)
            for i, line in enumerate(lines):
                if line.startswith("Relevant catalog:"):
                    lines[i] = "Relevant development-tool catalogs are host-local optional maintainer configuration, not product build/runtime dependencies. Verify current tool reachability before use.\n"
            raw = "".join(lines).encode("utf-8")
        if relative.endswith(".md"):
            text = raw.decode("utf-8")
            for literal in [str(root), root.as_posix()]:
                text = text.replace(literal, "<SWARM_SOURCE_ROOT>")
            # A source export intentionally does not copy private evidence/history.
            # Convert only links whose actual file target is not exported to text.
            def public_link(match):
                label, target = match.group(1), match.group(2)
                if ":" in target or target.startswith("#"):
                    return match.group(0)
                file_target = target.split("#", 1)[0]
                from urllib.parse import unquote
                required = ((root / relative).parent / unquote(file_target)).resolve()
                if required.is_relative_to(root) and required.relative_to(root).as_posix() not in {f["path"] for f in review["files"]}:
                    return f"{label} (maintainer-only record omitted from this review export)"
                return match.group(0)
            text = re.sub(r"\[([^\]\n]+)\]\(([^\s)]+)\)", public_link, text)
            raw = text.encode("utf-8")
        if raw != (root / relative).read_bytes():
            transformations.append(relative)
        prepared[relative] = raw
    output.mkdir(parents=True, exist_ok=False)
    for relative, raw in prepared.items():
        target = output / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        with target.open("xb") as file:
            file.write(raw)
    gaps = document_link_gaps({name: output / name for name in prepared})
    if gaps:
        raise ValueError("Candidate document links require review: " + json.dumps(gaps))
    result = {"schema_version": 1, "release_approved": False, "candidate_only": True,
              "scope": "Local source review export; exact non-document bytes, explicit Markdown locator/link transformations. Not a public upload or non-infringement/secret-free certification.",
              "transformed_documents": transformations,
              "public_test_fixture": "crates/swarm-core/tests/fixtures/google-test-private.pem; non-production test-only signing key",
              "files": [{"path": name, "bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest()} for name, raw in sorted(prepared.items())]}
    with (output / "source-export-report.json").open("x", encoding="utf-8", newline="\n") as file:
        json.dump(result, file, ensure_ascii=False, indent=2)
        file.write("\n")
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    result = export(Path(__file__).resolve().parents[1], args.output)
    print(json.dumps({"files": len(result["files"]), "transformed_documents": len(result["transformed_documents"]),
                      "release_approved": False}))
