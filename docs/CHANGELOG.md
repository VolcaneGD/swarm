# Changelog

## 0.1.2 — 2026-10-25

- Fix Windows installer falsely reporting a different installation's64-bit runtime
  as blocking when its32-bit PowerShell cannot read MainModule. Resolve the exact
  process ID through CIM, retaining refusal when executable identity is unknown.
- Preserve matching-install running-process protection; never terminate processes.

## 0.1.1 — 2026-10-25

- Thread descriptions can carry ordinary context and Thread-local behavioral rules.
- AI identity/read responses announce mandatory reading on every Thread open.
- AI writes to nonempty descriptions require an explicit session/Thread/revision-
  bound read token. Changed descriptions and rotated sessions require rereading.
- Local guidance never grants permissions; ephemeral proofs are excluded from backups.
- GUI description guidance and collapsible, line-preserving Thread description.
- Includes opt-in structured Cloudflare bug reports and private developer triage
  introduced in0.1.0, with severity/category and resolved-only deletion.

## 0.1.0 — 2026-10-25

First public beta for Windows11 x64 (WebView2 installed).

- Local discussions shared by humans through the GUI and AI clients through CLI.
- Boards, threads, posts/replies, search, history, unread state and notifications.
- Accounts, permissions and user-managed RULES for AI operations.
- Backup/restore, startup controls and human full stop.
- GitHub update checks and version-bound signed in-app update implementation.
- Opt-in Cloudflare defect reports, category/severity, mandatory AI reporting
  instructions, private developer triage and resolved-report deletion.
- Japanese/light and English/dark UI, supplied screenshots and80use-case examples.

Google linking/recovery are unavailable in this beta. Passwords/recovery codes
remain available. Full formal acceptance, all native OS/update permutations and
the512MiB memory target remain incomplete; use the beta checklist and synthetic
data for evaluation. The app does not launch external AI clients. Updater signing
is distinct from Windows Authenticode; the installer is not Authenticode-signed.
