# β公開前の必須検査と保留範囲

## 2026-10-24 — Minimum beta completion contract

The user's latest instruction is to minimize work and reach publication readiness
quickly. Stop expanding per-opcode/permutation fixtures. The17unchecked ROADMAP
rows include formal exhaustive coverage, explicit holds and post-publication work;
they are not17equal pre-publication blockers. Keep full formal acceptance honest,
but use the finite gate list below to choose work and report progress.

| Gate | Completion condition | Current disposition |
| --- | --- | --- |
| M1 RULES restart/recovery | One actual interrupted artifact fails closed in a freshly started fixed distribution runtime, explicit supported recovery restores original policy/Data and normal stop | COMPLETE: one actual prefix-write interruption and fresh-runtime supported recovery (maintainer-only record omitted from this review export). Whole-VM power loss/all stages remain formal coverage |
| M2 Public error sinks | Normal supported authentication/error/public outputs contain no credentials; known disclosure defects must be fixed | COMPLETE FOR MINIMUM COVERAGE: existing actual native credential/output15canaries (maintainer-only record omitted from this review export), password feedback8outputs (maintainer-only record omitted from this review export), and diagnostic outputs (maintainer-only record omitted from this review export). No genuine uncaught exception was executed. Under the October24 minimum-scope request, searching for an unreproduced uncaught exception is conditional/formal follow-up, not an unbounded beta prerequisite. This is reduced evidence coverage, not a claimed uncaught-error PASS or permission to ship a known leak |
| M3 Google beta offering | An actually usable/safe general-user offering, or an explicit product decision disabling the unverified Google GUI and execution routes in the first beta | COMPLETE: user selected disable; default build rejects Google before browser/challenge/pending auth, GUI shows inactive localized notices. Staged-binary refusal/password/recovery-code/backup smoke passed. Existing Google bindings preserved |
| M4 Distinct started-write entry | Determine whether a business mutation has a materially different stop/transaction entry; if so, one integrated write/stop/readback case only | Common listener Service gate and synchronous commit cover ordinary post.edit; existing direct-Service transaction/native stop, fixed-runtime credential commits and pending fence provide representative evidence. No per-RPC expansion for the minimum beta gate; full all-RPC contract remains formal |
| M5 Final candidate closeout | Match the final package to relevant changed components/evidence, record known limits and prepare reviewable release artifacts | COMPLETE: matching static-CRT release and NSIS candidate-20261024-beta-no-google,37artifact pins/301source snapshot, staged runtime smoke and live preview verified; English80use cases/3workflows and known limits prepared. Final evidence (maintainer-only record omitted from this review export). Publication remains a separate explicit instruction |

Current minimum gate work is **complete; technical beta blockers0**. Public release
is held for the user's explicit publication instruction. M1 is now actually observed;
M2's normal supported-output evidence is reused and its unexecuted uncaught path is
explicitly retained outside this first-beta minimum. M4 requires no additional
runtime matrix absent a specifically identified different entry/defect. Startup reboot/
tokens/cleanup, archived real-day expiry, minimum Codex/Gemini session/project and
the scoped native restore rollback/reauthentication/credential/cursor safety are
already evidenced and are not current beta blockers. Retain their exact candidate
and native/formal limitations. No whole R03/R07/R08/R18 or formal AC PASS is implied.

Only a newly observed defect, materially changed candidate or a named ordinary
safety condition can add another minimum blocker. Vague “other joins” cannot.
Do not count another completed helper or source-audit row as release progress.
The existing publication,512MiB,excluded-client/data and exhaustive-matrix holds
continue. No feature was disabled by this scope reconciliation alone.

2026-10-21現在の[公開判断](PUBLICATION_READINESS.md)はNOT_READY_FOR_PUBLICATION。
[全26ACの結果台帳](ACCEPTANCE.md)を更新し、R23／R24の記録・判断作業は完了したが、
正式AC合格や必須安全性の免除ではない。残17項目・公開保留・以下のユーザー決定を維持する。

決定日：2026-10-15。ユーザーは、直前の整理で「公開前に必須」とした範囲への着手と、「後回し候補」の当面保留を指示した。これは公開指示ではない。正式ACの条件・未達結果は保持する。

## 公開前に確認する範囲

| 範囲 | 残る受入条件と主な参照 |
| --- | --- |
| 資格・保存領域 | 管理者時の新規file／lock独立10観測・ACE・非継承・再取得は2026-10-20実VMで完了 (maintainer-only record omitted from this review export)、既存認証済み正常停止とversion境界を明記結合。GUI／rendererの未観測公開sinkに秘密が出ない条件はN02／N04／R06へ残す |
| 権限・非公開情報 | 資格・アカウント・ロール・個別権限変更後、旧資格と開いている画面で閲覧／操作が続かない。一覧・検索・履歴・変更・通知も対象。現Thread拒否後の実Windows通知センター新規配送抑止は2026-10-21部分確認 (maintainer-only record omitted from this review export)、他の未観測結合と全N02／N03／N04は維持 |
| RULES | 不正設定時に拒否し、編集・保存失敗・競合・変更反映が権限を広げない。中断後も安全に拒否／復旧する。N05／O04の安全性部分 |
| 基本利用・停止 | 初回設定からGUI／CLIの主要操作、保存・再起動、完全停止と復帰を確認。N01／N06／N10 |
| 削除・復元 | 確認／取消し、親子状態、削除済み本文の非公開、保持・採番の整合性。準備済み1日ケースの実経過native／private消去・復元拒否と公開AI readset／正常終了は2026-10-21部分完了 (maintainer-only record omitted from this review export)。未準備archive・履歴明示grant・残る権限／changes／通知と全N07は維持 |
| バックアップ復元 | 提供するなら、失敗時巻き戻し・再認証・旧人間／AI資格とcursor失効を確認。正常適用の既存証拠だけでは合格にしない。N13 |
| 導入・撤去 | 2026-10-20の保持済みVM候補で稼働中拒否・通常self-copy uninstall・元Data22file全hash／PATH／所有PATH・uninstall登録解除は確認済み (maintainer-only record omitted from this review export)。通知等の所有登録解除・通常画面／checkbox・最終候補での残条件はO02へ保持 |
| β対象・配布物 | 対応とする環境／AIで主要経路を通し、最終配布物・既知問題・制約と検査結果を照合して公開判断する。正式AC未達をPASSにしない |

## 当面保留する候補

- 全AIクライアントの導入行列の網羅。β対応とするAIの選定と最小導入確認は必須側へ残す。
- 全幅・日英・両テーマの網羅。初期設定と主要操作の利用可能性は必須側へ残す。
- Google連携・Windows通知の全配送／click・自動起動の全OS行列。
- WebView2未導入かつ完全オフラインでの導入。β環境条件を別途確定する。
- 最大規模・長時間・全障害タイミングの網羅。通常利用の安全性と既知のN14メモリ未達の開示は必須側へ残す。
- 実配信先・署名付き自動更新。既存の公開後作業という境界を維持する。

保留は、実装の削除・機能OFF・公開済み・検査合格を意味しない。Google／通知／自動起動／復元等が有効のまま候補に残る場合、その機能の情報漏えい・資格・データ保持・停止等の安全性を省略しない。公開前には、対象機能を検証して提供するか、実行経路も含め利用できない状態にするか、具体的なβ提供範囲を確定する必要がある。今回の指示だけでは後者の製品変更は行わない。

## 2026-10-18 B1決定：先行受入対象

ユーザーはWindows11 x64・WebView2導入済み、Codex＋Gemini CLIを先行受入対象とする案を採用した。他の既存対象AIは証拠を保持してβ確認未完とし、正式ACの網羅条件も維持する。有効な機能は変更せず必須安全性を確認してから提供する。機能OFF・公開・再試行予算追加は承認していない。

| 判断 | 採用した範囲・扱い | 根拠と残る条件 |
| --- | --- | --- |
| βの必須環境 | Windows11 x64、WebView2導入済み | 既存候補の検査環境と一致。未導入かつ完全offlineの行列は既存保留を維持。installerの稼働中拒否／通常撤去／Data保持の安全性は省略しない |
| βの優先対応AI | CodexとGemini CLIを最小の先行受入対象とする | 2026-10-14までの実session／応答したmodel切替／CLI操作の部分証拠がある。他の対象Claude Code／Hermes／Copilot／local LLMは未対応と断定せず、β確認未完として扱う。全対象の正式ACを合格にしない。現在の利用可否は未再測定 |
| 現在有効な機能 | 現状維持で必須安全性を検査してから提供する | Google／通知／自動起動の全行列は保留可能だが、有効な実行経路の安全性は残る。復元rollback等の未達があれば公開を止める。早期βを優先して未検証機能を無効化する選択は、具体的なUI／IPC／CLI経路の変更案と別の承認が必要 |

AIの根拠は[AI_SETUPの実行記録](AI_SETUP.md)。旧versionは観測時点の記録であり現行確認ではない。Codexの別client/accountでのunsupportedやGeminiの特定modelのavailability失敗を全体の対応不可と扱わない。除外4clientは選択肢へ戻さない。

既存6対象AIすべてを先行対象とする代替は選ばれなかった。対象を絞る決定は「未検査を合格」とするものではない。R09／R20は対象選定部分のみ解消し、最小導入とsession／projectの残条件は実証まで未完了。R22のoffline網羅は保留のまま、必須環境の判断材料は確定。

## 継続する制約

- 公開・配布はユーザーの「公開してください」まで保留。
- Antigravity／Cursor Agent・CLI／OpenClaw／OpenCodeはEXCLUDED_BY_USER。開発側試験へ戻さない。
- 既存Data／RULES／資格／fixtureと本番runtimeを保護する。
- 過去OS incidentの失敗・消費記録を保持し、新しい名前で失敗を隠さない。2026-10-23最新指示は通常の原因補修／未完了VM継続を承認しており、過去retry counterだけを追加承認gateにしない。現実の資源・同VM／全chain・guest資格／sessionとデータ保護を確認して進める。済み検査と同じ未修正障害は反復しない。
- 認証／security／privacyのnative入力は人間が行い、native削除・置換には実行時確認を要する。危険なOS障害検査は所有する使い捨てVMのみ。

正本：[ROADMAP](ROADMAP.md)、[PROJECT_SPEC](../PROJECT_SPEC.md)、[native検査](NATIVE_TEST_CHECKLIST.md)、[受入証拠](ACCEPTANCE.md)。現在の実行状態はHANDOVER (maintainer-only record omitted from this review export)を参照する。
