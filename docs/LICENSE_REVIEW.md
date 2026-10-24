# Locked Windows distribution license review

2026-10-24 updater supplement: the shipping notice bundle retains all813previous
texts and adds11texts covering minisign-verify0.3.0, reqwest0.13.5,
rustls-platform-verifier0.7.1, tauri-plugin-updater2.14.0, tempfile3.27.0 and
tokio-util0.7.19. All newly reviewed declarations are MIT/Apache-2.0; both
alternatives and upstream notices are retained. Existing five MPL archives are
unchanged. Current manifest pins current Cargo/npm locks and9verified resources;
the earlier October13 review below remains historical. No public release implied.

Reviewed2026-10-13 for the Windows11 x64 GUI/CLI/runtime and locally generated candidate. Publication remains held. This review records the actual included material and obligations; it does not turn native acceptance into PASS.

## Original material

The project manifests and LICENSE declare MIT. The owner confirms all original code/icon generation used Codex/ChatGPT, with no other AI services. See [asset provenance](ASSET_PROVENANCE.md). This refers to generated project output, not distribution of an OpenAI service/model. Original generation prompts are not needed build inputs. Existing license/copyright notices are retained.

## Third-party scope and implementation

Cargo normal/build/proc-macro Windows paths plus installed npm packages form a conservative superset. Build-only libraries are not assumed absent from generated output. All813collected package license/NOTICE/copyright texts are preserved byte-for-byte in THIRD_PARTY_NOTICES; package names/versions/source-relative filenames identify each section. All alternatives in OR/legacy slash declarations and all terms in AND expressions are retained. No GPL-family/noncommercial-only license appears in the reviewed declared-license set; unknown/new license tokens or package sets reject preparation until reviewed. This is a conservative inclusion policy, not a claim that every potential package contributes linked bytes.

MIT/MIT-0/ISC/BSD/0BSD/Zlib texts and copyright notices, Apache2.0 terms/NOTICE, Unicode3.0 notices, CC0/Unlicense alternatives, CDLA-Permissive2.0 and the CC-BY4.0 caniuse-lite build-data license are retained. caniuse-lite attribution: version1.0.30001815, original package authors/license retained, source https://github.com/browserslist/caniuse-lite and underlying compatibility data https://caniuse.com/. Including build-only texts avoids relying on dev flags as proof of exclusion. The60uninstalled optional packages contain no installed bytes on this host; their absence is not approval for a different platform/build.

All5MPL2.0 packages are covered: cssparser0.37.0, cssparser-macros0.7.1, dtoa-short0.3.5, option-ext0.2.0, selectors0.38.0. Exact locked .crate archives accompany the binary resources; all66archive members matched installed source, with original file notices and no modifications. SOURCE_AVAILABILITY explains obtaining/extracting them and MPL terms. This covers runtime/build/generated-code conservatively. [Mozilla's FAQ](https://www.mozilla.org/en-US/MPL/2.0/FAQ/) and [MPL2.0](https://www.mozilla.org/en-US/MPL/2.0/) require access to covered source and preserved notices; no Swarm MIT relicensing is applied to that source.

Bundled SQLite is the upstream amalgamation from libsqlite3-sys, which [SQLite documents as public domain](https://www.sqlite.org/copyright.html); the Rust wrapper's own license text remains included. No third-party fonts/photos are supplied as new original assets. Existing generated icon variants are retained.

Microsoft WebView2 remains separately licensed, outside Swarm MIT. The existing official signed Evergreen Standalone Installer is used unchanged, following [Microsoft's offline distribution procedure](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution). No fixed-version-runtime grant is substituted and no Microsoft installer binaries are modified. Actual clean offline installation/OS behavior remains O01; the current review does not establish that trial.

The9resource files are present in Tauri mapping, generated NSIS File/Delete instructions and a hash-verified33file local candidate. The build verifies exact resource/lock hashes without private review snapshots. See integration evidence (maintainer-only record omitted from this review export).

## Source publication scope

The reviewed source scope includes code, locks, SQL schema/migrations, assets, public examples, packaging Skill/openai.yaml and the9third-party resources. Runtime DBs, credentials, RULES, environment files, node_modules/target, private handover/logs/evidence and unrelated parent-repository changes are excluded. The explicitly allowed Google RSA key is a non-production public signature-test fixture; arbitrary PEM/private files remain excluded.

export_source_candidate.py produces a review-only tree with exact non-document bytes, replaces the local workspace locator, removes the host-local development catalog locator, and converts links to omitted maintainer evidence into explanatory text. Canonical instructions/specification/history are not edited by export. All transformed documents are enumerated, exported local links checked and file hashes recorded. Synthetic Windows test paths remain for regression coverage. The source export has been inspected for these known findings; literal scanning alone was not treated as a secret-free or rights guarantee.

Public upload, repository/asset release contents, signing and final distribution judgment remain dependent on the explicit owner publication instruction and held acceptance. Any changed locks, added dependencies/assets or new private files require renewed focused review.
