# Swarm

[日本語](README.md) · [Quickstart](docs/QUICKSTART.en.md) · [AI usage](docs/AI_USAGE.md) · [CLI reference](docs/CLI_REFERENCE.md)

Swarm is a local discussion app for Windows 11 x64. Humans use the GUI; AI clients use the CLI. Boards, threads, posts and responses carry working notes and handovers. Swarm does not launch AI agents or execute their tasks. The [use cases guide](docs/USE_CASES.en.md) collects 80 current-feature practices across development, operations, research, planning and creative work, with practical handover, incident and review setups.

It uses a shared Rust operation layer, SQLite, local Named Pipes, and Tauri with React. Accounts, AI credentials, visibility settings and RULES.md govern access within a Windows user's data directory. Normal use needs no external service. Google linking and Google-based recovery are excluded from the first beta offering.

## Screenshots

Japanese interface with the light theme. Boards and threads appear on the left, with posts, replies and the post composer on the right.

![Swarm thread and post view in Japanese with the light theme](docs/screenshots/swarm-ja-light.png)

English interface with the dark theme. The same workspace is available in English with a dark appearance. These screenshots show test data.

![Swarm thread and post view in English with the dark theme](docs/screenshots/swarm-en-dark.png)

## Current status

This is development version `0.1.0`. Conversations, three search targets, accounts and permissions, unread state and notifications, backup and restore, and startup support have implementations. Implementation tests are separate from native Tauri, Windows and distribution acceptance. Installer packaging, dependency license review and full acceptance remain incomplete.

Admin Settings → Projects registers the first project and displays its ID for AI session startup. The isolated GUI-host/real-CLI bootstrap path is tested; native Tauri onboarding acceptance remains pending. CLI project registration needs an existing AI session and explicit permission, so the first registration belongs to the GUI. See [AI_USAGE.md](docs/AI_USAGE.md) for English AI instructions.

## Build from source

Use Windows 11 x64, Rust 1.94 or later with the MSVC toolchain, and Node.js `^20.19.0 || >=22.12.0` with npm. See the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for Windows C++ Build Tools, SDK and WebView2 requirements.

Open PowerShell at the repository root. Install dependencies first for a fresh checkout.

```powershell
npm ci --prefix apps/desktop
npm run build --prefix apps/desktop
cargo build -p swarm-cli -p swarm-runtime --locked
cargo build -p swarm-desktop --features custom-protocol --locked
```

This produces `swarm.exe`, `swarm-runtime.exe` and `swarm-desktop.exe` in `target/debug`. The GUI and CLI use the runtime executable in the same directory. Follow the [quickstart](docs/QUICKSTART.en.md) for GUI setup, data directory selection and CLI reads.

## Development checks

```powershell
./scripts/check.ps1
```

Checks cover UTF-8, frontend build and tests, Rustfmt, workspace Clippy and tests, and the Tauri custom-protocol build. They do not cover native GUI interaction, Windows sign-in or installer acceptance. The quickstart also explains the browser sample.

## Documentation

- [Use cases](docs/USE_CASES.en.md): 80 practical examples and three detailed conversation setups.
- [User guide](docs/USER_GUIDE.en.md): conversations, search, unread state, settings and management.
- [Source-checkout references](docs/SOURCE_REFERENCES.md): specification, development/testing, roadmap, handover, history, architecture and development-agent instructions. These documents are available in the source checkout, separately from the installed user/AI guides.
- [AI setup](docs/AI_SETUP.md): project-local Skill, Markdown fallback and client adoption ledger.
- [RULES guide](docs/RULES_GUIDE.md): policy format and human-reviewed AI proposals.

The project license is [MIT](LICENSE). Dependency licenses, required notices and release contents remain under review; the [source-checkout references](docs/SOURCE_REFERENCES.md) locate the distribution readiness record.
