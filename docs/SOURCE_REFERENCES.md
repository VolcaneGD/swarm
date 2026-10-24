# Swarm source-checkout references

This locator is included with local Windows candidates. The maintainer documents
below belong to the source checkout; they are not included in an installed
candidate. Open these paths from the checkout root, not the installation folder.
No public source URL or published release is claimed.

| Source path | Purpose |
| --- | --- |
| `PROJECT_SPEC.md` | Current Japanese specification and formal acceptance states |
| `AGENT.md` | Instructions for agents developing Swarm, separate from product AI permissions |
| `docs/DEVELOPMENT.md` | Build and automated-check procedures and evidence boundaries |
| `docs/ROADMAP.md` | Settled implementation and verification work |
| `docs/HANDOVER.md` | Latest restart instructions and remaining acceptance gates |
| `docs/Development_Log.md` | Historical changes and verification records |
| `docs/ARCHITECTURE.md` | Architecture design; consult current progress separately |
| `docs/DISTRIBUTION.md` | Candidate preparation, dependency review and distribution gates |

## Client verification records

Priority session/project acceptance: `2026-10-19-client-context-matrix.md`.

The dated client records referenced by AI setup and model provenance are in
the source checkout's `docs/evidence/` directory. They are maintainer evidence,
not bundled installation instructions or promises of current model availability:

- `2026-10-14-native-codex-responding-transition.md`
- `2026-10-14-fresh-native-codex-report.md`
- `2026-10-14-native-codex-saved-report.md`
- `2026-10-14-opencode-native-export-compatibility.md`
- `2026-10-14-gemini-responding-model-transition.md`
- `2026-10-14-concurrent-cli-model-selection.md`
- `2026-10-13-gemini-opencode-recovery.md`
- `2026-10-13-all-cli-adoption.md`
- `2026-10-13-current-codex-adoption.md`
- `2026-10-13-current-codex-write-report.md`
- `2026-10-13-local-qwen-langgraph.md`

Installed user and AI procedures are available in the bundled
[English quickstart](QUICKSTART.en.md), [Japanese quickstart](QUICKSTART.md),
[CLI reference](CLI_REFERENCE.md) and [AI setup](AI_SETUP.md).
The installed package does not contain source build/test entry points, mutable
handoffs or historical evidence. A successful document preflight or installation
does not establish complete native/OS acceptance or third-party license clearance.
Publication and public distribution remain held until the owner explicitly says
「公開してください」.
