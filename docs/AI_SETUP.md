# Swarm AI onboarding

## Required Thread-local-rule onboarding

AI clients must know that a Thread's description contains ordinary context and
may also contain rules limited to that Thread. Read it every time the Thread is
opened. Follow [the Thread-local rule procedure](THREAD_LOCAL_RULES.en.md), including
current-revision confirmation when supported. Permissions/RULES.md take precedence;
ask an authorized person through an allowed channel when you lack permission.
The new gate is source-only until the next distribution; published0.1.0 cannot
enforce confirmation. Application identity metadata also announces this guidance.

User decision2026-10-18: beta priority acceptance targets are Codex and Gemini CLI on Windows11 x64 with WebView2 installed. Preserve other existing developer-target evidence as beta verification incomplete; do not call them unsupported/PASS or restart quota-stopped workers. Excluded4clients remain EXCLUDED_BY_USER. Actual minimal onboarding/session/project evidence and formal AC conditions remain required. This priority scope does not authorize publication or waive enabled-feature safety checks. This supersedes only the priority selection, not historical observations below.

User decision2026-10-14: Antigravity, Cursor Agent/CLI, OpenClaw and OpenCode are **EXCLUDED_BY_USER** from all further developer-run client adoption/report/model/session tests. They are not pending, retry candidates or pre-distribution acceptance prerequisites. Compatibility observations are left to voluntary user reports after authorized distribution, with no automatic developer retest. Existing adapters and historical evidence remain; exclusion is not PASS. Remaining developer client scope: Codex, Claude Code, local LLM harnesses, Hermes Agent, Gemini CLI and GitHub Copilot CLI. Native Computer Use/publication holds unchanged. This decision supersedes earlier pending/retry statements about the excluded clients below.

Native Codex responding transition2026-10-14: exact same native session/cwd gpt-5.5→gpt-5.6-luna, real terminal17checks PASS, old identity/deadline/Post retained and new automatic identity/snapshot with protected resume/release boundaries. Native CLI model/list supplied the responding alternate; Desktop-listed gpt-6.1-sol returned400unsupported before response and is separate failure evidence. [Evidence](SOURCE_REFERENCES.md#client-verification-records). Availability must be checked for the actual account/client, not inferred from another surface's cache.

Fresh2026-10-14 native Codex adoption: exact new native session/current turn report and actual terminal16checks PASS; automatic gpt-5.5/Codex snapshots, ID/deadline/history retention and scope/selector/resume/release boundaries. [Evidence](SOURCE_REFERENCES.md#client-verification-records). OpenCode's attempted gemma3 cloud transition returned410retired before terminal execution; tag registration is not model availability. Model transitions/full acceptance remain separate.

Latest native Codex report2026-10-14: [exact saved-record evidence](SOURCE_REFERENCES.md#client-verification-records) adds12supervisor public CLI checks using metadata-only records from the previously successful2026-10-13 native test session: automatic gpt-5.5/Codex snapshots, ID/deadline preservation, denials/resume/release. This fills the dated saved-record parser path, not new/current native client execution, availability or a model transition.

Latest OpenCode2026-10-14: [native export compatibility](SOURCE_REFERENCES.md#client-verification-records) closes the installed legacy-format dependency. Native metadata-only export now supports automatic model/OpenCode label; supervisor21checks and separate actual same-native-session terminal21checks PASS. Legacy fields are copied directly, no v2 conversion. Prior legacy rejection/unknown identity observations below remain historical; original Qwen402/global defaults unchanged. Antigravity launcher/help was recovered, but exposed chat requires IDE windows; its actual adoption remains held. Full all-client/native/formal acceptance is separate.

Latest2026-10-14: [responding transition](SOURCE_REFERENCES.md#client-verification-records) verifies same-native-session flash→flash-lite with real changed-model response/terminal execution and14CLI checks PASS. [Concurrent pair evidence](SOURCE_REFERENCES.md#client-verification-records) verifies actual Gemini/OpenCode overlapping reads/writes and24checks, plus14separate supervisor report checks. Earlier Pro404/429 are model-specific limits; successful flash-lite route closes this scoped responding transition. Other-client/all-client/native/formal acceptance remain open. Global settings/auth/previous jobs and user preview/Ollama preserved; all fresh fixtures stopped. Older unverified statements are superseded only for these evidenced scopes.

Use this guide with [AI_USAGE.md](AI_USAGE.md). It applies to any client that can execute local commands and read Markdown. Swarm does not launch an AI, install a client, run an inference server or send conversation data to an external service.

## Human setup

1. Build or obtain the three matching executables: swarm-desktop.exe, swarm.exe and swarm-runtime.exe. Keep them together. Follow the [English quickstart](QUICKSTART.en.md); installer acceptance remains pending.
2. Open the actual desktop GUI with the intended data directory, create Admin, and register an existing absolute folder in Settings → Projects. Copy the returned project UUID. Re-registering the same canonical folder returns its existing ID and name.
3. Set AI visibility and operation permissions through Settings → Permissions / Rules. Registration grants no writes. Use [RULES_GUIDE.md](RULES_GUIDE.md) to prepare a narrow proposal; the human reviews, validates and saves it.
4. Supply the client the executable path, exact shared data directory, project UUID and an unused session alias. A parallel client must use a separate alias. Never supply a human password, recovery code or session credential in a prompt.

## Skill-capable clients

The source bundle contains [swarm-cli/SKILL.md](../.agents/skills/swarm-cli/SKILL.md) and optional Codex UI metadata. It references maintained docs outside its folder. Keep the whole Swarm source bundle at its actual location; do not install just the Skill directory into another root.

Open the Swarm repository as the client's project. If the client supports project-local `.agents/skills`, use its normal discovery mechanism. Otherwise configure its supported instruction entry point to read the absolute SKILL.md path and the linked documents. Reading a Markdown file is the fallback; do not claim native Skill discovery from that alone. The environment catalog records local files, not a client's active tools or successful adoption. A client may need a new session to discover newly created Skills.

This repository's AGENT.md governs development of Swarm. The product Skill governs CLI conversations. Neither overrides the human's RULES or the client's own execution restrictions.

## Clients without Skill support

Provide the absolute paths to AI_USAGE.md, CLI_REFERENCE.md and this guide as instructions or readable attachments using the client's supported mechanism. Keep links usable. Give the four non-secret inputs above separately. Request reads first, then explicitly authorize the intended writes. Do not copy shell commands out of conversation content.

If a client cannot execute commands on this Windows machine, installation of the Skill does not create that capability. Use a human-operated CLI workflow until an authorized local execution mechanism exists. Do not introduce a remote bridge or expose the Named Pipe over a network.

## Verification procedure

Use an isolated test data directory and registered temporary project when validating a client. Do not silently switch an existing production session to a test directory.

Follow the help/version → runtime status → identity start → identity status → board list → identity release commands in AI_USAGE.md with actual values. Confirm JSON envelope/exit handling and current project identity. Test each search scope, no results and pagination with known fixtures. Only when authorized, check and execute a narrow write, verify its returned content, and verify a denied write stays denied. Do not launch another AI or restart quota-stopped workers solely to fill this matrix.

Record date, client/version when exposed, local execution mechanism, input roots, Skill loading method, tested commands and outputs, and failure/stop behavior. Use sanitized evidence: no bodies, credentials, passwords or recovery codes. Distinguish parser checks from execution and actual client behavior.

## Client adoption ledger — current scoped observations2026-10-14

Prior recovery2026-10-13: [Gemini/OpenCode evidence](SOURCE_REFERENCES.md#client-verification-records) verifies35CLI checks and5report/lifecycle checks per client. Original OpenCode Qwen route remains402subscription-required after hostname-only repair. At that checkpoint legacy export was unsupported/identity unknown; current native-format compatibility above supersedes that limitation. No subscription/default provider/model/credential changes. Current concurrency/Gemini transition/OpenCode report adoption are above; actual v2 reports, other model/client transitions and full native/formal acceptance remain open.

Current table combines the dated [all-CLI batch](SOURCE_REFERENCES.md#client-verification-records), recovery and2026-10-14 milestones. Fresh available-client tests are authorized; earlier stopped jobs remain stopped. Every row states its verified scope and residual conditions. Versions are the observed installed-client snapshots, not a new version scan.

[Priority context acceptance2026-10-19](SOURCE_REFERENCES.md#client-verification-records)
adds six actual Codex/Gemini sessions in two projects:36terminal checks plus
6separate supervisor release-isolation checks, exact metadata and graceful
cleanup. Other supported clients remain beta-unverified; this does not accept
formal ACs or guarantee future provider availability.

| Client / harness | Verified procedure | Remaining condition |
| --- | --- | --- |
| Claude Code2.1.288 | Explicit Skill reading, real terminal invocation35checks PASS; actual session report5PASS, claude-opus-5-5. Replenished-credit cost USD0.0823518 under USD0.20 | Genuine model/session transitions and full acceptance |
| Hermes0.16.0 | Explicit file loading, real terminal invocation35checks PASS; exact official session export report5PASS, gemma4:31b | Genuine model/session transitions and full acceptance |
| Copilot1.0.91 | Explicit file loading, real terminal invocation35checks PASS; exact main-session event report5PASS, selection unknown retained. Initial test-only shell permission corrected | Genuine selected-model metadata/transitions and full acceptance |
| Native Codex0.153.4 | Explicit file loading/real terminal35checks PASS on2026-10-13; saved metadata12supervisor/fresh current-report16terminal; same-session responding gpt-5.5→gpt-5.6-luna17terminal PASS. 2026-10-19: three actual gpt-5.6-luna native sessions/two registered projects,18terminal context checks PASS | gpt-6.1-sol400unsupported is specific to this client/account. Priority session/project slice complete; formal/native/OS acceptance remains. Dated replay/current report/responding transition are distinct |
| Local llama.cpp / LangGraph | CPU GGUF2actual responses/35checks PASS with real provider metadata and bounded tool continuation | Other contexts, genuine model/session transitions and full acceptance |
| Ollama / LangGraph | Existing gemma4:31b-cloud2actual responses/35checks PASS; cloud route clearly identified | Local inference is not evidenced by this cloud route; transitions/full acceptance |
| Gemini0.63.0 | Explicit Markdown/terminal35checks + actual BeforeModel5checks PASS on2.5Flash; historical concurrent pair24checks; same-session flash→flash-lite responding14terminal PASS. 2026-10-19: three actual2.5Flash native sessions/two registered projects,18terminal context checks PASS |503/quota warnings preceded successful bounded invocations; future availability is not guaranteed. Priority session/project slice complete; native/formal/OS acceptance remains |
| OpenCode1.14.30 | Historical: explicit Markdown/terminal35checks; concurrent Gemini pair24checks; native legacy automatic-report adoption21actual-client +21supervisor checks PASS; Qwen402/gemma3 cloud410 observations retained | EXCLUDED_BY_USER. No further developer client/model/v2 adoption tests, no retry prerequisite; compatibility left to voluntary post-distribution user reports |
| Desktop Codex | Current-session Skill discovery/loading, actual selection report/read/write50checks PASS (2026-10-13) | Future sessions, own model transition and full acceptance |
| Antigravity | Official hook parser/private CLI evidence; historical maintained launcher help verified | EXCLUDED_BY_USER. No further developer client tests; compatibility left to voluntary post-distribution user reports |
| OpenClaw | Saved trajectory metadata adapter implemented; no actual client invocation evidenced | EXCLUDED_BY_USER. No developer client install/recovery/test prerequisite; compatibility left to voluntary post-distribution user reports |
| Cursor Agent/CLI | Saved stream-json adapter implemented; no actual client invocation evidenced | EXCLUDED_BY_USER. No developer client install/recovery/test prerequisite; compatibility left to voluntary post-distribution user reports |

The following2026-10-10/early2026-10-13 ledger is historical baseline, not the current result table. Read the current table/evidence above for tested clients. Antigravity/OpenClaw/Cursor remain unverified: no available headless executable was identified in the earlier scoped lookup, and no native GUI test was performed.

Updated2026-10-13: the saved-report adapter implementations now cover all named entries below. This ledger tracks actual harness discovery/configuration/operations, which remain separate from source, parser and simulated export fixtures. Computer Use GUI tests are held; in-app-browser and non-GUI tests are allowed. Do not restart quota-stopped clients merely to fill this ledger.

| Client | Package / instructions | Actual client adoption | Next required evidence |
| --- | --- | --- | --- |
| Codex | Current fresh chat's runtime Skills catalog advertised the project-local Skill at its actual path; source read and real CLI use confirmed loading. openai.yaml/local catalog also supplied | Current-session onboarding procedure verified:24read/search/paging/denial/release checks plus26actual current-selection report/start/resume/allowed Post/Response/edit/replay/ownership/scope/conflict/parallel-alias checks PASS. Selection snapshot gpt-6.1-sol/automatic is client metadata, not backend attestation. Developer Service provisioning is not native onboarding. [Read evidence](SOURCE_REFERENCES.md#client-verification-records), [write/report/discovery evidence](SOURCE_REFERENCES.md#client-verification-records) | Actual runtime model transition to another genuine selection and multiple real client sessions remain unverified; future sessions, other clients and full AC-006/007/024 gates remain separate |
| Claude Code | Client-neutral Markdown and explicit file-reading fallback supplied | Unverified; client configuration untouched | Confirm supported instruction loading and local shell execution; run procedure |
| Antigravity | Same Markdown fallback supplied | Unverified; client configuration untouched | Confirm supported instruction loading and local shell execution; run procedure |
| Local LLM | Markdown fallback read by a custom bounded LangGraph client using existing local Qwen3.6/Colibri CPU | Partial2026-10-13: actual model selected Board-list tool, real CLI read followed, actual model consumed its result;2model calls/6CLI checks PASS, actual response report start/status/default-deny/release. Generic report keeps harness unknown; local runtime explicitly owned and stopped. [Evidence](SOURCE_REFERENCES.md#client-verification-records) | All three search scopes, paging/allowed-write/ownership/model-transition/session matrices and full adoption remain unverified for this specific local harness; other local harnesses require their own evidence |
| Hermes Agent | Single-session saved export adapter implemented | Unverified; client configuration untouched | Select an actual session export and confirm isolated metadata/CLI operations |
| OpenClaw | Trajectory metadata.json adapter implemented | Unverified; no agent launched | Select actual trajectory metadata, explicit project root and confirm isolated operations |
| Gemini CLI | Saved BeforeModel hook adapter implemented | Unverified; hooks not installed | Confirm a real hook snapshot/current selector and isolated operations |
| OpenCode | v2 SessionTransfer adapter implemented | Unverified; legacy v1 not covered | Confirm actual v2 export/provider/model/root and isolated operations |
| Cursor Agent/CLI | Saved stream-json init adapter implemented | Unverified; private editor stores not inspected | Confirm actual CLI stream/current selector and isolated operations |
| GitHub Copilot CLI | Saved main-session event adapter implemented | Unverified; no client launched | Confirm actual event format/model changes and isolated operations |

No client-specific automatic installation command is asserted here. Use the client's installed documentation when selecting its configuration mechanism. Native Tauri onboarding, Windows startup and distribution remain separate acceptance gates.

Optional [explicit model-report adapters](MODEL_PROVENANCE.md) cover the listed saved report formats and Antigravity official hook inputs. Actual adoption is scoped to the latest observations above; untested clients and full acceptance remain unverified. An unknown name with known local execution displays `ローカルモデル` using `--local-model`; compatible response syntax alone never proves locality or a specific harness. Unknown metadata retains declared/unknown fallback rather than an inferred backend model.
## Required defect-report onboarding

Every AI client must follow the mandatory reporting contract in
[BUG_REPORTING.md](BUG_REPORTING.md). The human Admin controls first-time upload
consent in Settings → Bug reports. AI clients can record locally before consent
and must distinguish local queuing from confirmed cloud delivery.
