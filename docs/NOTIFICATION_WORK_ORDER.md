# 通知・購読・未読の実装準備

更新: 2026-10-09 JST。原目的はROADMAP第6章の継続。正本はPROJECT_SPEC.md §9.2／§9.3とAC-009／AC-012／AC-020。本書は作業分解と期待結果。段階1の実装範囲はevidence (maintainer-only record omitted from this review export)に記録し、残る配送・GUI・全受入を完了扱いしない。

## 再開条件

2026-10-09再開後：本チャットでは内部ブラウザー操作toolが公開され、公式Browser APIでSettings区切り線の実表示確認を完了した。通常幅／400px・日英・両テーマ、Disclosure／runtime／自動起動・権限不足・キーボードをsampleで確認。詳細はevidence (maintainer-only record omitted from this review export)冒頭。通知core段階へ進める。実Tauri受入は別に残す。次段落は再開前の制約記録。

Settings区切り線の表示確認を先に終える。現状はstyles.css修正、関連DOM78件、TypeScript／Viteが成功。実ブラウザー／実TauriはNOT_RUN。browser制御／node_replが公開されず、既存APIの接続診断も呼べない。bootstrap／browser troubleshootingと共通復旧手順を確認し、別のUI操作機構で代替しない。詳細はevidence (maintainer-only record omitted from this review export)。本書の準備はこの表示受入を解除しない。

## 確定要件と現在の接続点

- 人間の現在選択された有効ログインだけへ配送。GuiHostはgui-stateのDPAPI保存にSelectionのaliasを保持する（crates/swarm-gui-host/src/lib.rs）。複数の保存済み資格があるだけで全員へ通知しない。
- SessionBridge::gui_requestからHumanBridge::requestへ進み、既存Brokerで現在の人間資格を検証する（crates/swarm-runtime/src/session.rs、crates/swarm-adapter/src/human.rs）。段階2で通知先の現在選択をDPAPIから読み、同じBrokerで再取得・検証する接続を追加した。OS配送consumerは後続。独立BrokerやGUIのDB直接更新を追加しない。
- GUI終了後もruntimeが配送を担当する。通知無効化、ログアウト、アカウント無効化、資格失効、権限喪失、完全停止を配送時にも再確認する。
- 2026-10-09ユーザー回答：選択後の新着だけを通知する。ログアウト中・別アカウント選択中の履歴を切替後に配送しない。未読保存は独立したまま維持する。旧選択の保留配送を取り消す。
- Responseの直接返信先をpostsのthread_id／reply_toから特定し、そのauthor_idを不変の人間account IDと照合する。display_idやモデル名で通知先を判定しない。
- 現在のchangesは投稿作成と同一トランザクションで記録される（crates/swarm-core/src/service.rs）。通知の候補はcommit済みの変更だけから作る。rollback・確定済み同一要求再送で通知を増やさない。
- changes公開結果は本文・内部sequenceを含まない（crates/swarm-core/src/changes.rs）。通知専用の内部判定と公開APIを区別し、非公開対象のID・件数・本文をrendererへ渡してから隠す方式を使わない。
- SQLite schemaは版7（crates/swarm-core/src/store.rs）。段階1の購読・投稿単位既読・通知設定3表に加え、段階2で通知選択・outboxを追加した。既存thread_pinsは独立して維持する。OS送信後のcrash／置換・集約状態は後続段階。
- 2026-10-09ユーザー回答で、画面に表示した投稿だけを既読にすることが確定。別ページ・折りたたみ内・画面外・別アカウント・changes取得では既読にしない。飛び飛びの既読を扱う投稿単位保存とpost.read.markの入力は正本§9.3へ具体化した。GUIの実表示検知方式は未実装。

## 実装順と所有範囲

段階3：Thread別の集約ID・固定対象一覧・再試行状態を既存app_metadataの単一versioned journalへ保存する。schemaは版7のまま、新しい製品依存なし。最大200件、最初の対象投稿時刻に指定窓を加えた期限で固定し、後続窓の投稿は次のbatchへ分離する。再検査で資格／権限／生存／設定／既読に合わない対象を除外する。失敗時は5秒から最大5分まで指数的に延ばす内部機構を追加したが、製品consumerは未接続。2026-10-09の追加回答で集約窓は2秒に確定した。通知クリックは現在資格と閲覧権限を確認して対象Threadへ移動するというユーザー回答を得た。

段階4のplatform部品：既存lockfile内のwindows 0.62.2を直接依存へ追加し、root専用のAUMID・protocol・Start Menu shortcutを所有検査して登録する。WinRT ToastGenericとprotocol activation、安定batch Tagによる置換、本文初期非表示、送信後の履歴照合を追加。隔離identityの無音実送信・件数変更・1件置換・削除を確認した。通常runtime consumerとGUI起動引数・既存GUIへの引渡し・再認証・現在権限による移動は後続。記録は実API検査 (maintainer-only record omitted from this review export)。

runtimeのtrusted deliver_notification_batchはSharedService guardを呼出元が保持し、選択gateを再取得・再検査・sink送信・確認記録まで保持する。sink内からruntime IPCへ戻らない。受理された完全停止と選択変更はこの区間の後に進む。まだ製品listenerにconsumerを追加しておらず、この契約をmock sinkから実OS受入へ拡張しない。証拠は集約検査記録 (maintainer-only record omitted from this review export)。

2026-10-09の段階2の成果：現在選択資格の再取得、GuiHostの選択保存との共有ロック、確定投稿と同じtransactionの候補挿入、永続配送IDと現在資格／権限／生存／設定による準備を実装。記録は検査証拠 (maintainer-only record omitted from this review export)。作業順2の短時間集約、OS送信／置換／失敗再試行、送信前後のcrash受入は未完了。次はこの配送契約とWindows API／identity登録を具体化する。以下の全作業を完了扱いしない。

1. core: アカウント別の購読・投稿単位既読・通知設定の保存と現在権限による取得を追加する。schema移行は原子的に行い、既存データ・pins・changes cursorを維持する。Rendererが他人のaccount IDを指定して書けない操作契約を正本で具体化する。
2. core／runtime: 現在選択された有効人間資格とcommit済み変更から通知候補を作り、現在権限・削除状態で投影する。通知初期有効、本文抜粋初期無効。Responseと購読の両条件を満たす同一投稿は重複しない。短時間集約と再起動後の配送状態を永続化する。
3. GuiHost／adapter: 選択・切替・ログアウトと保留要求の境界を安全に接続し、既存私有資格経路を維持する。新操作はcore引数検証・人間allowlist・GuiHost検証・型・bridgeを同時にそろえる。AI向け経路で人間購読・既読・設定や他人の通知を変更できないことを検査する。
4. platform／runtime: Windowsのローカル通知をruntimeから配送する。Tauriのrenderer lifetimeへ依存させない。platform部品に続き、通常runtime worker／native sinkと通常GUI起動時の登録を接続した。隔離identityでGUIホストなしのworkerから無音実送信・outbox確認・未読保持を検証済み。実Tauri終了後配送・click・配布／オフライン受入は別に確認する。
5. GUI: Thread購読、未読表示、通知設定と本文抜粋設定を接続する。Viewport内に実表示した投稿の識別子を捕捉し、資格／Thread／表示形式変更で古い報告を無効化する。取得成功だけで既読にしない。両会話表示方式・深い返信・ページング・スクロール復元で検証する。

## 要件から先に定める検査

2026-10-09 GUI接続：Thread購読Toggle Button、未読Badge、Settings Checkbox Groupと明示保存、実表示本文の安定ID報告を実装。旧accountの書込要求はgui_notification_manageの期待account照合で拒否し、主体は現在の保存済み資格から推定する。取得・最大No.・Thread openで一括既読にしない。非表示・modal背後・inert・sticky header背後、旧ページ／表示形式の観測を除外し、失敗時は明示Retry。12件の新規DOM検査、既存隔離Named Pipe検査の拡張とsample実表示で確認した。500件取得後もMain Threadの未読992件を保持し、通知無効・Settings modal中も未読を保持した。ユーザー指定の上限メッセージ欄内移動・赤色維持・青系ダークも検査済み。実Tauri、Windowsバナー／click、配布登録と全ACは別に残る。通知文言は現在日本語のまま、表示言語の永続化と連動が残る。詳細はGUI検査記録 (maintainer-only record omitted from this review export)。

|入力・操作|期待結果|証拠|
|---|---|---|
|人間Aの投稿に翌日Response、表示ID変更|不変account IDでAを通知対象にする|core、実IPC。AC-009は全受入と区別|
|AとBを保存、現在選択A|B向け新着をAへ出さない|実DPAPI／Named Pipe／runtime|
|切替中・保留資格・無効化・logout・失効|未検証資格や旧アカウントへ出さない|境界同期、実資格|
|購読と直接返信が同時成立、同要求再送|同じ投稿を重複計上・配送しない|core、restart fixture、mock sinkと実通知を分離|
|Thread開く、500件取得、Viewportには一部だけ|実表示した投稿だけ既読、ページ外・折りたたみ内は未読|core＋DOM＋実表示|
|No.1と50を表示、No.2〜49は未表示|中間投稿を未読のまま保存|coreの永続化・再起動|
|同じThreadをBで表示、Aでchanges.get／wait|Aの既読を進めない|core／実IPC|
|集約中に閲覧権限喪失・親／投稿削除|配送直前の再検査で対象・件数・本文を除外|coreとruntime同期|
|本文抜粋初期設定、明示有効、後に権限失効|初期は種類・件数のみ。有効時も現在権限で検査|payload unit＋実Windows通知|
|短時間連続commit、rollback、完全停止|確定分だけ集約。停止後に送信しない、OS処理中との境界を記録|実SQLite／runtime、実OS別途|
|GUI閉じる、runtimeは継続|選択資格・設定・権限が有効なら通知|実Tauri終了＋実Windows通知|
|OS通知無効・送信失敗・process終了前後|失敗を成功扱いせず、再試行で重複する境界を明示|実OSと故障fixture|

DBとOS配送の原子性は保証できると仮定しない。永続outbox、同一通知の置換識別子、配送記録の順序とcrash時の意味を実装前に定める。実送信成功とユーザーが表示を目視したことは別の証拠にする。

## Windows参照資料と未決事項

Microsoftの[通知API overview](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/)は、ローカル通知を実行中のアプリコードから送る方式として定義し、unpackaged Win32向けにもAppNotificationManagerを案内する。これはRust／Tauriでの採用・依存検証を済ませた証拠ではない。配布・登録要件と既存Rust実装への適合を確認し、外部通知サービスを追加しない。

表示設定と通知文言の連動を追加した。人間本人の言語・テーマをapp_metadataへ保存し、未保存時はWindows defaultsを使用する。batch準備時に現在の保存言語を読み、未保存時はWindows表示言語で文言を選ぶ。隔離SQLite／実Named PipeとWindows defaults API、DOM／sampleで検査済み。詳しくは表示設定検査 (maintainer-only record omitted from this review export)。

未完了: click／GUI終了後配送の実Windows／Tauri受入、配布登録、購読・設定・表示既読・表示設定の実Tauri受入と両言語の通知目視。購読・設定・表示検知GUIは上記のとおり実装とsample検査済み。通知clickのstrict起動引数・DPAPI intent・既存GUI引渡し・現在資格／閲覧権限確認・対象Thread移動とRetryは実装し、隔離実Named Pipe／DPAPIとsample browserで検証済み。clickでは完全停止を無条件解除しない。通常consumer／native sink／通常GUI起動時の登録を接続し、2秒集約と隔離identityのworker無音実送信を検証した。現在の仕様にない挙動を黙って確定しない。既存の確定範囲内で内部方式を具体化し、受入が変わる製品判断は必要な時点で提示する。

全AC／実通知／実Tauri／Installerは本書によってPASSにならない。本番データ・RULES・Run登録・親Gitを保護し、試験は隔離領域で行う。
