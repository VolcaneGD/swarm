# Swarm development and testing

Audience: maintainers and AI development agents. Follow [AGENT.md](../AGENT.md), the Japanese [PROJECT_SPEC](../PROJECT_SPEC.md), current HANDOVER (maintainer-only record omitted from this review export) and [ROADMAP](ROADMAP.md). This English guide explains the current build/test entry points; it does not supersede requirements or acceptance gates.

Use [ACCEPTANCE.md](ACCEPTANCE.md) to locate partial evidence and the remaining observations for each formal AC. Source/report links are not fresh executions or acceptance passes.

## Acceptance bookkeeping CLI

Use the queue in [REMAINING_WORK](REMAINING_WORK.md) before choosing another acceptance task. Python3.10+ standard library only; no install, network, subprocess, GUI, OS registration, credentials or fixture access. From any directory, pass this script's absolute path; the workspace is bound to its own location, never the caller's working directory.

```powershell
python scripts/acceptance_status.py status
python scripts/acceptance_status.py check
python scripts/acceptance_status.py check --json
```

`status` reports ROADMAP totals, original25residual closures separately, current source line numbers and grouped prerequisites. `check` rejects missing/ambiguous queue mappings, unclassified open rows, stale current count markers, invalid UTF-8/replacement characters and broken local links in current sections. Historical counts are preserved. Current markers in ROADMAP, PROJECT_SPEC, HANDOVER and REMAINING_WORK must reflect actual checked/open/total counts. Completing or splitting a residual also requires manually reconciling its queue mapping with the preserved acceptance conditions; automation never decides completion.

Optional source/candidate preservation checks:

```powershell
python scripts/acceptance_status.py snapshot --output C:/Users/Unknown/.codex/project-runs/swarm-cd29d4a35ea531ee/acceptance-source-before.json
python scripts/acceptance_status.py check --baseline C:/Users/Unknown/.codex/project-runs/swarm-cd29d4a35ea531ee/acceptance-source-before.json --candidate target/distribution/candidate-20261016-scope-guard
python -m unittest discover -s scripts -p test_acceptance_status.py
```

Choose a new output filename for each genuine baseline; exclusive creation refuses overwrites. Snapshot hashes allowlisted product sources/locks only (crates/apps, excluding dependencies/build outputs); it contains paths/hashes, not file bodies. Comparison detects additions/deletions/changes and wrong workspace identity. It proves equality to that snapshot, not an earlier historical state. Explicit candidate checking verifies the existing report's files/installers and unpublished flags; it does not rebuild, refresh or authorize release. No default baseline/candidate is guessed and no private fixture or AppData tree is scanned. Only snapshot writes its explicitly named new file. Reports go to stdout, errors to stderr/exit1; no automatic retries, document edits, PASS adoption, test runs or watchers. Run only the affected source tests/builds when code changes justify them, through the existing focused commands/check.ps1 below.

## Environment and source build

The target is Windows11 x64. Cargo declares Rust1.94 minimum, edition2024; use the MSVC toolchain. The current Vite7 toolchain needs Node.js `^20.19.0 || >=22.12.0` and npm. Use Windows C++ build tools/SDK and WebView2 prerequisites from the [README](../README.en.md). Preserve Cargo.lock and package-lock.json. The locked build may fetch missing dependencies; offline builds require an already populated cache.

Run at the repository root:

```powershell
npm ci --prefix apps/desktop
npm run build --prefix apps/desktop
cargo build -p swarm-cli -p swarm-runtime --locked
cargo build -p swarm-desktop --features custom-protocol --locked
```

Use npm ci on a fresh checkout or when its dependency state needs rebuilding. The resulting debug executables are target/debug/swarm.exe, swarm-runtime.exe and swarm-desktop.exe. CLI/GUI locate the runtime beside themselves. A custom-protocol Rust build does not constitute an installer or native interaction test.

## Source responsibilities

Windows candidate builds additionally verify distribution/third-party against current locks and its exact resource manifest. These resources are source inputs; private target/distribution review snapshots are not required for a fresh source build. Maintainer-only prepare_third_party_bundle.py can regenerate a reviewed resource set from immutable review inputs; its cache-dependent integration tests explicitly skip when those inputs are absent. verify_third_party_resources.py and its tests remain runnable from clean source. Keep SQL migrations, the explicitly public signature-test fixture and the packaging Skill/example inputs in source exports. Current source inventory reports missing literal Rust and packaging inputs; source export never approves publication.

| Location | Responsibility |
| --- | --- |
| crates/swarm-core | Shared Service operations, SQLite and permissions/policy |
| crates/swarm-platform | Windows platform primitives and protected local state |
| crates/swarm-ipc | Protocol/transport-facing support |
| crates/swarm-adapter | Private credential candidates/journals and AI/human session bridges |
| crates/swarm-runtime | Runtime lifecycle, Named Pipes, backup/restore and notifications |
| crates/swarm-cli | CLI parser/executable and real-binary integration tests |
| crates/swarm-gui-host | Human GUI host, credential handling and operation bridge |
| apps/desktop/src | React UI and DOM/component tests |
| apps/desktop/src-tauri | Native Tauri commands/application |

For design context read [ARCHITECTURE](ARCHITECTURE.md); use current code and HANDOVER for implementation status. GUI and CLI must use shared operations rather than independent SQLite writes. RULES.md remains the policy authority. Never expose credentials to renderer/CLI output or infer hidden objects through diagnostic errors.

## Automated checks

```powershell
./scripts/check.ps1
```

Run the check and Windows-candidate entry points sequentially. Both hold `target/frontend-build.lock` while generating and embedding frontend assets; a competing pipeline refuses before changing `dist`. Do not bypass the lock with a concurrent direct frontend build. The Tauri build script watches the whole `dist` directory and requires its index for custom-protocol builds, including recovery from a previously empty asset snapshot.

The script checks strict UTF-8/no replacement characters in its discovered source files, builds the frontend, runs frontend tests, checks Rustfmt, runs workspace all-target Clippy with warnings denied, runs workspace tests, then builds Tauri with custom-protocol. It stops on failures. Hidden project Skill/YAML files need their own UTF-8/schema validation because the script's rg globs do not cover them.

For focused changes choose the affected checks; these are executable commands, not a requirement to run every check after a text edit:

```powershell
npm run typecheck --prefix apps/desktop
npm test --prefix apps/desktop -- src/SearchDialog.test.tsx src/ProjectSettingsPanel.test.tsx
cargo test -p swarm-cli --test documentation --locked
cargo test -p swarm-cli --test onboarding --locked
cargo test -p swarm-cli --test concurrency --locked
cargo test -p swarm-core --test policy --test rules_editor --locked
cargo test -p swarm-runtime --test restore_preparation --locked
cargo fmt --all -- --check
cargo clippy -p swarm-core -p swarm-cli --all-targets --locked -- -D warnings
```

The documentation test checks reference examples, AI examples and matching bilingual quickstart commands using the real parser, and real binary help/version. Placeholders passing parsing are not authenticated successful operations. The onboarding test uses temporary roots, the actual GUI host/runtime Named Pipe and CLI binary; it does not drive a native Tauri window. The policy test validates/saves the maintained example in isolation and exercises actual permission boundaries.

The concurrency test requires a freshly built swarm-runtime.exe beside swarm.exe. It creates20 aliases via actual CLI, synchronizes20 creation processes, replays unchanged creation keys, releases identities and stops its directly owned runtime through the human route before durable reads. Captured output is drained while processes run. This is not actual model/client adoption.

The long restore deadline check is opt-in; run only its bounded parent:

```powershell
cargo test -p swarm-runtime --lib restore_transport_tests::restore_over_three_hundred_seconds_preserves_accepted_result --locked -- --ignored --exact --nocapture
```

It holds the real Service gate for310 seconds and uses production300-second client/server deadlines, with a360-second child bound. It verifies accepted application/result lookup after timeout and old-credential rejection/reauthentication. It does not exercise native GUI Cancel or full-stop controls. Do not invoke its ignored worker standalone.

Some tests are Windows-only. A skipped platform module is not a passed Windows result. Ignored child-process workers are invoked by their bounded parent tests; do not run all ignored tests or call workers standalone. The opt-in Windows notification test changes isolated OS registration and is not part of routine silent verification. Inspect its exact prerequisites and scope before using it. Never remove ignore guards merely to increase counts.

The maximum restore resource observation is separately opt-in. On Windows x64, verify at least10GiB free temporary-drive space and8GiB free physical RAM first. Run only this parent:

```powershell
cargo test -p swarm-runtime --lib restore_apply::tests::maximum_restore_memory_observations --locked -- --ignored --exact --nocapture
```

It constructs an exact1GiB database image with bounded test-only metadata blobs and unchanged schema/RULES, then streams its checksum-valid archive. Separate owned child processes exercise apply and rollback with300-second worker bounds. Reports contain timing/memory counters and state only. Peak counters are process-lifetime values; fixture construction, preview and preparation occur in the parent and are excluded from child measurements. This debug/storage fixture is not a release/native GUI performance guarantee or OS disk-full acceptance. Default checks continue to run0/32MiB observations and skip the1GiB parent. Do not invoke memory_observation_worker directly.

## Packaged CLI/runtime concurrency

First build and stage a fresh candidate using [DISTRIBUTION](DISTRIBUTION.md). The test-only override is restricted to a canonical directory under this workspace's target/distribution. It selects the staged CLI and its sibling runtime; absent the override, normal debug test binaries are used.

```powershell
$candidatePrevious = $env:SWARM_TEST_CANDIDATE_DIRECTORY
try {
    $env:SWARM_TEST_CANDIDATE_DIRECTORY = (Resolve-Path target/distribution/0.1.0-windows-x64-candidate-verified).Path
    cargo test -p swarm-cli --test concurrency --locked
    if ($LASTEXITCODE -ne 0) { throw 'Packaged concurrency failed' }
} finally {
    $env:SWARM_TEST_CANDIDATE_DIRECTORY = $candidatePrevious
}
```

This starts only an owned isolated runtime and20 real CLI creation processes, verifies request-key replay, posts/responses/ownership/public reads and durable SQLite integrity, and stops the owned runtime through the human path. It does not install the candidate, launch GUI, modify production settings or demonstrate AI-client/model adoption. Native and clean-machine loader/installer acceptance remain open.

## Browser sample and native GUI

```powershell
npm run dev --prefix apps/desktop -- --port 5194
```

Open http://127.0.0.1:5194/preview.html. This explicit port overrides the script's default5173; strictPort refuses an occupied port. Reuse an existing preview owner instead of launching duplicates. The sample uses preview.tsx fixtures and does not register real projects, persist the real DB, authenticate real accounts or exercise Windows integration. DOM/sample checks cannot establish native acceptance.

For native trials follow the [quickstart](QUICKSTART.en.md) with a dedicated data root and all three executables colocated. Normal GUI startup also initializes Windows startup/notification registrations; a temporary data root alone does not isolate OS effects. Prepare an isolated Windows test account/VM or another explicitly scoped OS test environment before acceptance that changes those settings. Preserve existing runtime owners, full-stop state, production RULES/data and active jobs. Closing GUI leaves runtime running.

## Real client context matrix

`scripts/client_context_matrix.py` automates one explicitly scoped real-client
acceptance slice. It starts six actual Codex/Gemini CLI invocations in two new
projects in one fresh developer Data root, captures exact metadata fields,
checks session/project isolation through public CLI, releases aliases and stops
only its owned runtime. It uses the existing client authentication and can make
provider calls; it does not install clients, change global settings, schedule
runs or retry a failed supervisor invocation automatically.

```powershell
cargo build --offline --manifest-path scripts/fixtures/client-context/Cargo.toml --target-dir target
python scripts/client_context_matrix.py --help
python scripts/client_context_matrix.py inspect --run target/client-context-20261019/Run
```

For an authorized **new unmet condition**, run mode requires `--run` pointing to
an absent directory below this workspace's `target`, and `--helper` selecting
the exact `target/debug/swarm-client-context-fixture.exe` just built. Existing
run paths are rejected. Native invocation bound240seconds, CLI call20seconds,
barrier180seconds and helper self-stop360seconds; no automatic supervisor retry.
A new path does not authorize resetting an incident budget. Driver/hook modes
are internal to that invocation. A failed provider call is not product acceptance.
PASS requires successful native receipts and authenticated graceful cleanup.

`inspect` reads existing receipts and actual terminal completion events without
launching clients or product processes. It is evidence validation, not a new
product test or historical cryptographic command signature.

The fixed-root `scripts/fixtures/elapsed-once` helper belongs to the same exhausted
elapsed-seed incident. The original extra attempt failed before runtime startup.
The user explicitly approved extension 2 once; `--extension-2` selects its exact
approval receipt/root and preserves the consumed original entry. Its single
execution succeeded and is now consumed. Do not execute either seed entry again,
change roots or remove guards. A real one-day baseline exists, runtime normally
stopped, no watcher. Earliest follow-up:2026-10-20 23:18:08JST, same baseline/Data,
never reseed. See result and boundaries (maintainer-only record omitted from this review export).

`scripts/fixtures/native-drain` is the fixed H1-only response-backpressure probe.
Its original native attempt missed the4-second window; the separately human-
approved extension1 subsequently passed committed-response drain and is consumed.
Do not rerun either entry or infer transaction-drain acceptance. Both existing
output-directory guards refuse replay. Read
the exact diagnostic result (maintainer-only record omitted from this review export) before use.

The 2026-10-19 completed run (maintainer-only record omitted from this review export) already
covers the priority clients' same-model sessions/two-project separation. Reuse
that evidence; do not launch the same matrix merely to increase test counts.
Formal AC, GUI/OS integration and other supported clients remain distinct.

The prepared retained-VM host entry is `scripts/vm_retained_continuation.ps1`.
Read-only `-ShowPlan`, `-ValidateApprovalOnly`, and `-InspectResources` do not
access Hyper-V or start a VM. The explicitly approved extension was consumed by
the6144MB startup-allocation failure. Do not run `-ExecuteApproved` again.
Historical plans remain in `target/vm-retained-20261020/plan.json` and
`plan-ps51-fixed.json`; exact executed source is preserved beside them. The source
now has a resource guard before attempt consumption, local-only verified. No new
execution plan/authority has been created after that failure.
The proposed guest operation removes its retained Installed through normal NSIS
uninstall, verifies original Data/PATH/owned registrations, and is not native UI,
independent elevated ACL or full OS acceptance. Read
actual failure, resource guard and remaining boundary (maintainer-only record omitted from this review export).
Local checks: `python scripts/test_vm_retained_continuation.py`. Stage-specific
cases skip when actual approval/attempt evidence must be preserved; never remove
those receipts to run a test. Unicode approval is ASCII-escaped for PS5.1.

## Evidence and stop conditions

Record the actual command, date/time, build mode, platform, temporary root handling, exit status and meaningful assertions. Keep Service/SQLite, real Named Pipe/binary, DOM/sample, native GUI, Windows lifecycle and installer evidence distinct. Mark failed, not-run and environment-constrained results explicitly. An AC remains open until its own required evidence is obtained.

For restore, retain separate gates for OS disk exhaustion at Safety/RULES/DPAPI storage, maximum input/resource measurements, >300s waiting, actual GUI cancellation/full stop, arbitrary write-time termination/power loss and native picker/application/rollback/reauthentication. SQLite page-limit failure is not OS disk-full evidence; a fixed commit-boundary kill is not arbitrary interruption; sample navigation is not native picker evidence. Installer/WebView2/PATH/uninstall/license and real Windows sign-in remain separate work.

Use temporary fixtures and the existing managers. Never fill a production drive, kill an owner-managed runtime, invoke another client/model to consume quota or modify production policy just to obtain evidence. Report RESTORE_RECOVERY_REQUIRED and preserve uncertain results; do not bypass a blocked runtime or manually rewrite credentials/SQLite.

Manual edits use apply_patch and strict UTF-8 readback. Update ROADMAP only for evidenced work, append Development_Log with JST timestamps, and refresh the English latest HANDOVER envelope. This repository is currently untracked inside a dirty parent Git repository: do not stage/reset unrelated files. Skill changes require the installed catalog maintenance workflow; ordinary documentation edits do not require another catalog scan.
