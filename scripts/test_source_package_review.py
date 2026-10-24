"""Source inventory boundaries, not public-source approval."""
import json
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch
from source_package_review import classify, inventory, write_review


class SourceReviewTests(unittest.TestCase):
    def test_readme_screenshots_are_explicit_but_local_zenn_assets_never_export(self):
        for name in ("docs/screenshots/swarm-ja-light.png", "docs/screenshots/swarm-en-dark.png"):
            self.assertIsNone(classify(Path(name)))
        for name in ("docs/private-zenn/swarm-introduction.md", "docs/private-zenn/assets/swarm-zenn-eyecatch.png", "docs/screenshots/private.png"):
            self.assertEqual(classify(Path(name)), "outside-explicit-source-scope")

    def test_literal_compile_inputs_are_reported_without_promoting_private_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root = self.fixture(directory)
            source = root / "crates/core/src/lib.rs"
            source.parent.mkdir(parents=True)
            source.write_text('const A: &str = include_str!("schema.sql");\n'
                              'const B: &str = include_str!(".env");', encoding="utf-8")
            (source.parent / "schema.sql").write_text("fixture", encoding="utf-8")
            (source.parent / ".env").write_text("private-canary", encoding="utf-8")
            result = inventory(root)
            self.assertEqual(result["build_input_gaps"], ["crates/core/src/.env"])
            self.assertNotIn("private-canary", json.dumps(result))

    def test_only_explicit_packaging_skill_and_cargo_config_are_selected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = self.fixture(directory)
            allowed = {".agents/skills/swarm-cli/SKILL.md", ".agents/skills/swarm-cli/agents/openai.yaml",
                       ".cargo/config.toml", "docs/cli-context.example.json"}
            denied = {".agents/private.md", ".agents/skills/other/SKILL.md", ".cargo/credentials.toml"}
            allowed |= {"distribution/third-party/MPL-2.0.txt", "distribution/third-party/selectors-0.38.0.crate"}
            allowed |= {"crates/core/src/migrations/001_initial.sql"}
            allowed |= {"crates/swarm-core/tests/fixtures/google-test-private.pem"}
            denied |= {"crates/core/real-private.pem"}
            denied |= {"distribution/third-party/private.json"}
            for name in allowed | denied:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("fixture", encoding="utf-8")
            selected = {f["path"] for f in inventory(root)["files"]}
            self.assertTrue(allowed <= selected)
            self.assertFalse(denied & selected)

    def fixture(self, directory):
        root = Path(directory)
        for name in ("Cargo.toml", "Cargo.lock", "LICENSE"):
            (root / name).write_text("fixture", encoding="utf-8")
        return root

    def test_allowlist_excludes_runtime_secrets_history_and_builds(self):
        with tempfile.TemporaryDirectory() as directory:
            root = self.fixture(directory)
            paths = ["crates/core/src/lib.rs", "apps/desktop/src/View.tsx",
                     "docs/USER_GUIDE.md", "scripts/check.ps1", "icon.png",
                     "apps/desktop/.env.local", "crates/core/RULES.md",
                     "docs/HANDOVER.md", "docs/Development_Log.md",
                     "docs/evidence/private.json", "target/release/private.exe",
                     "apps/desktop/node_modules/a/index.js", "data/swarm.db", ".agents/a.md"]
            for name in paths:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b"private-body-canary")
            result = inventory(root)
            selected = {file["path"] for file in result["files"]}
            self.assertEqual(selected, {"Cargo.toml", "Cargo.lock", "LICENSE", *paths[:5]})
            self.assertNotIn("private-body-canary", json.dumps(result))
            self.assertFalse(result["release_approved"])

    def test_output_outside_boundary_and_existing_output_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            root = self.fixture(directory)
            with self.assertRaises(ValueError):
                write_review(root, root / "public.json")
            output = root / "target/distribution/review.json"
            write_review(root, output)
            initial = output.read_bytes()
            with self.assertRaises(ValueError):
                write_review(root, output)
            self.assertEqual(output.read_bytes(), initial)
            self.assertNotIn("target/distribution/review.json",
                             {file["path"] for file in inventory(root)["files"]})

    def test_link_refused_before_output_creation(self):
        with tempfile.TemporaryDirectory() as directory:
            root = self.fixture(directory)
            with tempfile.TemporaryDirectory() as foreign:
                try:
                    (root / "crates").symlink_to(foreign, target_is_directory=True)
                except OSError:
                    self.skipTest("OS does not permit fixture symlink creation")
                output = root / "target/distribution/review.json"
                with self.assertRaisesRegex(ValueError, "symlink/junction"):
                    write_review(root, output)
                self.assertFalse(output.exists())

    def test_windows_reparse_refused_without_symlink_privilege(self):
        with tempfile.TemporaryDirectory() as directory:
            root = self.fixture(directory)
            output = root / "target/distribution/review.json"
            original = Path.lstat
            def reparse(path):
                return (SimpleNamespace(st_file_attributes=0x400, st_mode=original(path).st_mode)
                        if path.name == "LICENSE" else original(path))
            with patch.object(Path, "lstat", reparse):
                with self.assertRaisesRegex(ValueError, "symlink/junction"):
                    write_review(root, output)
            self.assertFalse(output.exists())


if __name__ == "__main__":
    unittest.main()
