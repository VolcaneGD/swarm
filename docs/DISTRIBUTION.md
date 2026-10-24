# Swarm distribution readiness

2026-10-24: the repository is privately staged at VolcaneGD/swarm. Real update
checking and signed native installation are implemented; see [UPDATES](UPDATES.md)
for the fixed GitHub feed, version-bound signing and asset preparation command.
The current notice bundle adds11MIT/Apache texts for6updated/new dependencies;
the five existing MPL source archives are unchanged. Source updates do not
authorize public visibility, release uploads or portfolio deployment. Historical
candidate counts and hashes below apply to their dated builds.

This English maintainer guide records preparation and scoped local installer evidence, not a published release or full installer acceptance. The project declares MIT and now includes [LICENSE](../LICENSE). Contributor attribution does not establish rights to third-party dependencies/assets; review those separately.

Public release/distribution is explicitly held until the user says「公開してください」. Local source preparation and isolated verification do not authorize publication.

## Local dependency audit

Current locked license/source review is in [LICENSE_REVIEW](LICENSE_REVIEW.md), with the owner's [original asset provenance](ASSET_PROVENANCE.md). All813notice texts and five locked MPL source archives now have9verified distribution resources in distribution/third-party. Build preflight verifies them without private review caches. Tauri/NSIS mapping and local candidate33file hashes passed; installed use and publication remain separate. The source-export helper prepares a review-only tree with explicit document locator/link transformations; canonical files are preserved.

Notification icon source is configured separately from Tauri/NSIS icons. The existing 128px PNG is embedded in the platform binary; native registration publishes it in private notification-state storage and sets the owned AppUserModelId IconUri/background. Missing legacy fields can be added; conflicting values/assets are refused. No loose installation resource is needed. Source/debug checks do not update the installed candidate; banner/settings rendering and installed cleanup remain N11/O02 gates. See verification (maintainer-only record omitted from this review export).

Source-publication scope can be inventoried without copying or publishing files:

```powershell
python -X utf8 scripts/source_package_review.py --output target/distribution/source-review-20261013.json
```

The fresh JSON records relative paths, bytes and hashes selected by an explicit source path/suffix scope, plus exclusions. Generated caches, evidence, session handovers/logs, hidden agent configuration, RULES, environment files and runtime databases are excluded. Links/junctions are refused. Selection is a review candidate only: manually review contents, private paths, examples, third-party source/notice duties, assets and source rebuildability before approval. No source archive, Git stage/commit or public upload is performed. The report retains release_approved:false; it is local review evidence, not a notice bundle or a secret-free guarantee.

Run at the repository root with an existing Python3.11+ installation and populated local Cargo/npm caches:

```powershell
python -X utf8 scripts/distribution_audit.py --output DEPENDENCY_AUDIT.json
```

The output must not already exist. The tool creates it exclusively, reads local Cargo metadata with --locked --offline and a Windows x64 filter, then scans npm's lock/installed package metadata and top-level license/notice filenames. It does not install packages, contact a registry, bundle files or publish anything. Paths are local evidence locators; do not publish the report blindly as a finished notice bundle.

2026-10-11 audit (maintainer-only record omitted from this review export):332 Cargo metadata entries and283 npm lock entries. All have declared license metadata. There are72 entries without a located top-level notice, including60 uninstalled platform-specific optional npm packages. Missing located notices require review; they are not automatically license violations or safe exclusions. Cargo's package list can include entries outside the filtered resolve graph, and dev/build dependencies are included. Determine actual shipped reachability before making a distribution decision.

The first all-target offline metadata command needed an uncached windows-sys0.52.0. Filtering to the supported Windows x64 target resolved the local audit without downloading or changing dependencies. This does not establish complete metadata for other targets. Nested notices, bundled SQLite and Windows/WebView2 redistribution conditions need separate review. The generated report always states release_approved=false.

## Packaging gates

October13 preparation combines797 local notice files plus16 pinned upstream supplements into target/distribution/notice-review-assembled-20261013, retaining candidate provenance and verifying813 output hashes. scripts/assemble_notice_review.py refuses lock/package/hash/path drift and output reuse. Separate selectors-source-review-20261013 binds the exact cached source archive to Cargo.lock and all22 installed files, with a separately sourced Mozilla MPL text; it does not assign a missing package-specific notice or claim compliance. Five focused Python tests passed. See assembly evidence (maintainer-only record omitted from this review export). Actual notice applicability/branch/attribution/source-location and shipping integration remain open. Further acceptance inspection is deferred while the user prioritizes adapter implementation.

Resumed October12 review (maintainer-only record omitted from this review export) collects797 exact local notice files from331 potential production-graph Cargo packages and the283-entry npm lock superset, tracks normal/build/proc-macro paths and records8 immutable upstream license supplements for6 missing-text crates. See the sanitized summary (maintainer-only record omitted from this review export). The72 local notice gaps remain explicitly recorded; these supplements are candidates for applicability/attribution review, not automatic clearance. Reproduce with `python -X utf8 scripts/collect_distribution_notices.py --help`; upstream retrieval is a separate explicit `python -X utf8 scripts/fetch_notice_supplements.py --help` operation. Each output must be fresh and under target/distribution. Publication remains held.

The 2026-10-12 locked dependency review (maintainer-only record omitted from this review export) checks direct npm ranges and declared Cargo Rust minimums without changing locks or downloading packages. Reproduce with `python -X utf8 scripts/dependency_policy_check.py --help` and choose fresh output paths. All615 entries have license metadata;72 top-level notice gaps and actual shipped reachability remain unresolved. Windows prerequisites (maintainer-only record omitted from this review export) records verified MSVC/SDK/WebView2 and the current build.

| Item | Verified preparation | Remaining work |
| --- | --- | --- |
| Project license | MIT manifest and LICENSE file | Review publication/asset ownership scope |
| Dependencies | Local metadata/notice locators and explicit gaps | Select applicable license branches, collect required texts/attributions for shipped components |
| Executables | Matching release x64 GUI/CLI/runtime, installed24-file hash agreement, native onboarding/shared CLI readback and20 actual packaged CLI processes passed | Complete native matrix and client adoption |
| Tauri bundle | Base bundle.active=false; explicit distribution overlay generates an unsigned current-user NSIS candidate | Required notices/docs, installation/update/uninstall and final release review |
| WebView2 | Generated installer includes the offline payload; downloaded Microsoft payload signature Valid | Test a clean disconnected Windows11 x64 VM without WebView2 |
| PATH/startup | Installed owned PATH append/type preservation verified; update left PATH unchanged; dedicated startup disabled through native GUI | Actual Windows sign-in and uninstall PATH behavior |
| Update/uninstall | Actual scoped install/update exit0,24 hashes and22 retained data-file hashes; PS5 empty guard fixed and actual active guard23 | Uninstall, RULES preservation and failure scenarios |
| Publication | No release sent | Source/assets/secret allowlist, full acceptance and final publication decision |

## Build and inspect a local candidate

Run `python -X utf8 scripts/package_windows_candidate.py --check-documents` for a
read-only check of mapped local Markdown links. The build script also runs it
before compiling/bundling. Missing package targets reject the candidate; code
examples and external links are excluded, while fragment anchor validity is not
checked. Current17 mapped documents have zero missing local file targets;
SOURCE_REFERENCES.md locates source-only maintainer files without broken bundle
links or copying history. See native/package evidence (maintainer-only record omitted from this review export).

Use Windows x64, PowerShell7+, existing Rust/MSVC/npm tools and populated caches. The build script requires8GiB free workspace-volume space and refuses preexisting Rust flag overrides. It sets static-CRT flags for all three release executables in its own process and restores the original environment afterward.

```powershell
pwsh -NoProfile -File scripts/build_windows_candidate.ps1
python -X utf8 scripts/package_windows_candidate.py --output target/distribution/0.1.0-windows-x64-candidate-verified
python -X utf8 scripts/test_package_windows_candidate.py
```

The staging destination must be fresh and under target/distribution; choose another name for a repeat. Existing outputs are never overwritten. A failed stage can leave a partial folder without package-report.json; do not treat it as verified. -SkipInstaller builds executables only. The script uses locked/offline Cargo compilation, but Tauri bundling can download its managed NSIS/WebView2 inputs. Build-time downloads differ from the required disconnected target installation test. See [Tauri Windows installation](https://v2.tauri.app/distribute/windows-installer/).

Each newly staged installer is independently copied into that candidate's
installers directory. package-report.json points to the retained copy using a
workspace-relative path; source_path records the original build locator. Verify
the retained copy's hash before execution. Later builds cannot replace this copy.
The installer snapshot is separate from the files installed into the application
directory. Older reports that reference target/release/bundle/nsis remain
historical and do not receive this protection retroactively.

The explicit overlay apps/desktop/src-tauri/tauri.distribution.conf.json maps CLI/runtime beside the GUI and supplies LICENSE, both READMEs, CLI_REFERENCE, both quickstarts/user guides/use cases, AI/RULES/model-report guides, SOURCE_REFERENCES, real examples and the project Skill/openai.yaml at their original relative paths. Current-source NSIS and installed byte/hash verification passed. It selects current-user NSIS, English/Japanese language selection, the existing ICO and offlineInstaller. Required shipped dependency notices and real client adoption remain incomplete.

The installer hook source evidence (maintainer-only record omitted from this review export) covers planners and NSIS syntax. Later host install/update evidence (maintainer-only record omitted from this review export) adds actual registry/PATH and native verification plus the PS5 guard correction. Historical October11 candidates predate current source.

The verifier records SHA-256, x64 PE headers, GUI/runtime windowed subsystems, CLI console subsystem, GUI icon resource, imported DLLs and CLI help/version. It rejects separately redistributed VC runtime imports. OS UCRT/APISet imports remain allowed on the supported Windows11 baseline; [Microsoft documents UCRT as an OS component on Windows10 and later](https://learn.microsoft.com/en-us/cpp/porting/upgrade-your-code-to-the-universal-crt?view=msvc-170). It does not install or run the GUI. Package report release_approved is always false.

2026-10-11 candidate evidence (maintainer-only record omitted from this review export) and machine report (maintainer-only record omitted from this review export) are historical. Current final stage is target/distribution/candidate-20261012-native-ui-final. Generated files are local evidence, not public release assets. Exact disposable VM/baseline and user-authorized separate host fixture are verified in HANDOVER. Clean offline/absent-WebView2 and OS failure gates remain open; never fill the host drive or use production data for those tests.

See [ACCEPTANCE](ACCEPTANCE.md), [ROADMAP](ROADMAP.md) and HANDOVER (maintainer-only record omitted from this review export) for preserved gates. No GitHub release/upload, production data mutation or external AI worker launch was performed. Owned test installation/PATH append and startup disable are explicitly evidenced above.
