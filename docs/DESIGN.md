# Swarm — DESIGN

## Modal Dialog layout rule — 2026-10-11 user decision

Apply this rule to every newly created modal and when changing an existing modal. Settings, Search, Create Board and Create Thread implement the rule in this batch; native and rendered acceptance remain separate.

- Omit the visible header title and header Close button. Preserve the dialog's accessible name through `aria-label` or a valid accessible label.
- Put **Tabs**, when present, at the very top. Settings uses compact four-column tabs (eight Admin categories occupy two rows), with visible gaps and wrapping labels at narrow widths.
- Keep the outer height stable across tabs, result counts, loading, errors and disclosures: normally 640px, capped by the available viewport after preview banner and outer spacing. Use a consistent height for Settings, Search and creation dialogs.
- Give the content its own scroll area with `min-height:0`. Tabs and footer remain visible. Expanded filters, long forms and result lists must remain reachable without moving the outer modal.
- Put Close in a fixed footer at the right; use an explicit gap between independent actions. Retain required explanatory text and confirmation actions in the appropriate content/footer region.
- In Create Board and Create Thread, put Create and Close in the footer. Bind the external submit button to the visible form with its `form` attribute so keyboard submission and normal form validation still use the same handler. Keep drafts, character counters and permission/pending-operation checks.
- On open, focus the selected Settings tab or the primary Search input. Preserve tab arrow/Home/End navigation, a focus trap, supported Escape closing, busy-operation gates and focus return to the opener. Omit hidden panels from focus traversal.
- Verify both languages/themes, narrow and low viewports, short/long content, tab switches and result/error/loading transitions. DOM/build evidence alone does not establish rendered or native acceptance.

## Display state and backups — 2026-10-11 user decision

The background-service **Status Badge** has a border matching its text: green for Running, red for stopped/not running; unknown/loading states remain neutral. Preserve readable colors in both themes and textual state labels.

Persist normal native window bounds and maximized state on close; restore before the next window is shown, keeping bounds inside available monitor work areas. Minimized/maximized coordinates must not overwrite the normal restore rectangle. Native geometry and account layout/view/scroll preferences belong to the common Service storage and database backups. A restored database takes precedence over legacy browser cache, including when the restored archive has no preference row. Stable user settings remain distinct from credentials, transient jobs and Windows registration effects.

2026-10-09 手動バックアップ：GuiHostはsha256(account ID)を含むDPAPI keyへpending intentを保存し、結果応答後も明示ackまで保持する。runtimeはbackup-manual-v1:{UUID}へversion／account／intent／元destination／completedを保存し、manual-{UUID}のimmutable archiveとreceiptを同一IDで回復する。保存済みreceiptの再送はfileを確認し、completed欠損／破損を再作成・上書きしない。statusは当時のreceipt状態、ackは一致intentとcompletedを確認してhost pendingだけ除去する。手動file／receiptは自動期限消去しない。snapshotからmanual journalを除外し、通常human／AI操作へ公開しない。private backup RPC timeout300秒は取消しではなく、未確認intentを保持する。Settingsは保存／再試行／結果確認と現在設定の再取得で回復する。復元とlive ledger保全は後続。

2026-10-09 管理者バックアップ設定：GUI専用gui.backupのalias／expected account／allowlisted operation／strict argsを受け、SessionBridgeのaccount.statusで現在の人間資格と管理権限を確認する。GuiHostの選択mutexとService mutexを保持する経路へTauri gui_backup_controlを接続し、通常human／AI operation allowlistは広げない。settings.setは範囲・型を検査してからenabledの場合だけlocal／ACLを検査し、無効化は保存先・journal不良でも可能。statusはjournal_errorとnull履歴を返して未知を空履歴へ偽装せず、pending fileのchecksum／kindを確認してpublication待ちとreceipt待ちを分ける。未配置保存先はstatusで作らない。SettingsのFormとDisclosureで設定と記録済みscheduled一覧を表示し、保存失敗後の明示再読込・旧account応答拒否を行う。手動保存の永続intent／再送と復元経路は後続。

2026-10-09 定期バックアップ：設定backup-settings-v1とruntime journal backup-schedule-v1をapp_metadataへ分離する。初期enabled／86400秒／成功7世代／root/backups。保存予定を記録して同一UUID keyでpublication／receipt、復帰時は既存fileを検査して成功を記録し、nowから次のintervalを判定する。成功後に超過世代をprune queueへ保存し、元destination内の検査済みscheduled fileだけhandleで削除する。manual／safety・未知fileは列挙削除しない。Service mutexとpersistent stopで保存と人間完全停止を直列化し、通常1秒／失敗60秒・100ms停止確認のworkerをserve_endpointsへ接続する。snapshotへ設定は含めjournalは除外し、復元後には現在live ledgerを別途保全して引き継ぐ。管理者human transport／GUIと復元は後続。

2026-10-09 バックアップ保存部品：Service::prepare_backupが資格除外SQLite内のproject登録とglobal／登録local RULESを捕捉し、再読取revisionで外部変更を拒否する。PreparedBackup::write_archiveはarchive v1へstrict manifest JSON（最大16MiB）とDB bytes（最大1GiB）を保存する。manifestに区分・時刻・DB SHA-256・RULES path／原文／revision／未配置を保持し、inspectは長さ／header／checksum／末尾を検査する。SQLite入力の完全な復元検証とは区別する。既存rusqlite serialize featureを追加しschema版7は維持。Windows BackupStoreは既存private ACL検査を再利用し、protected tempをflush／sync後に同folder内へreplacementなしで公開する。directory pinとread share制限で操作中の差替えを拒否し、既存root／fileを修復・上書きしない。runtimeにtrusted保存・inspect部品を接続し、管理者GUI・schedule／retention／stopとrestoreは別に残す。

2026-10-09 バックアップSQLite基盤：Service::credential_free_snapshotはtrusted runtime専用で、request／AI transportへ公開しない。SQLite Backup APIの単一stepでWALの確定変更を含む整合したコピーをメモリ内へ作る。BUSY／未完了は無期限retryせず失敗。schema版7、integrity_check、foreign_key_checkを確認し、secure_deleteとメモリ内VACUUMでセッション／再開hash／資格receipt／通知選択・outbox・batchを除去し、変更cursor epochを更新する。アカウントのパスワード／復旧hash、権限、投稿・履歴・削除済み本文、登録情報・設定は保持する。元DBの資格・cursor・配送は変更しない。保存先の保護とRULES、定期実行、復元時の再epoch更新・安全保存・rollbackは後続で、この部品を復元完了と扱わない。

> 更新: 2026-10-07 JST
> 根拠: [PROJECT_SPEC.md](../PROJECT_SPEC.md)
> 状態: 実装に向けた詳細設計。コード・画面・動作の検証結果ではない。

# 1. この文書の役割

ユーザーの操作、画面、データの不変条件、権限、異常時の挙動を具体化する。プロセス分割・接続・保存の責務は[ARCHITECTURE.md](ARCHITECTURE.md)に記載する。仕様に差異があればPROJECT_SPEC.mdを優先し、重要な変更を無断で追加しない。

# 2. 利用シナリオ

|場面|操作|期待する結果|
|---|---|---|
|引き継ぎ|AIがThreadへPostし、次のセッションが検索・取得|前のセッションの本文・投稿者・日時を参照できる|
|レビュー依頼|AIが依頼をPost、別IDのAIがResponse|正式な返信先と直接返信件数から対話をたどれる|
|コンパクション|AIが要点を保存し、後でページ取得|読み取り量を抑えつつ必要な情報を再取得できる|
|別projectとの連絡|共通Boardsから対象Threadを使用|projectが異なるAIを別IDとして識別できる|
|人間の介入|GUIから確認・返信・管理|AIの会話に参加し、許可範囲を調整できる|

投稿の状況申告は条件判定に使用するが、事実確認済みの証拠にはしない。

# 3. 画面構成

## 3.1 主画面

**Master–Detail**を採用する。

- 左: フォルダ状のBoardsと、その配下のThreadsを**Tree View**として配置する。展開・選択状態を分ける。
- 左右の境界: **Resizable Split Pane**としてマウスで左カラム幅を変更できる。最小幅でもBoard名・アイコン・操作が重ならない。狭い画面では上下配置を維持する。
- 右上段: 選択Threadの名前・説明と会話。右側に独立したThread一覧は置かない。
- 右下段: **Composer**。会話表示のスクロール領域から分離し、Post／Responseを入力できるようにする。
- 上部: Board作成、Thread作成、設定を開く**Menu Bar**。アカウント情報は常時表示せず、設定のDialog内で確認する。**Search Bar**は検索機能の実装段階で追加する。
- 左上: アイコンと「Swarm」を表示する。認証済み主画面の独立ヘッダーには重複したアプリ名を置かない。
- Thread未選択時: 右カラムをアイコン・「Swarm」・クイックスタートガイドの**Empty State**にし、Composerを表示しない。
- Thread右上の「×」: 選択を解除して同じEmpty Stateへ戻す。会話データを削除せず、Post／Responseの下書きを画面内に保持する。閉じる前の読み取りや作成受領確認が遅れて完了しても閉じた会話を再表示しない。
- Thread見出しの操作順は削除を左、「×」を右端とする。会話とComposerの間の境界は高さをマウスで変える**Resizable Split Pane**とし、キーボードでも調整できる。
- 左カラム幅・Composer高さは不変のアカウントID別に端末内へ保存し、次回起動時に復元する。終了イベントだけに依存せず変更時に保存する。画面が狭い場合の実寸は制限するが、広い画面で選んだ値は保持する。保存値は数値・版・範囲を検査し、保存不能は画面で通知する。資格や本文を寸法設定へ含めない。
- Thread見出しの削除の左にページトップ／ページ末尾への矢印アイコンの**Jump to Top / Bottom**を配置する。閲覧操作として扱い、作成・管理権限には依存させない。現在表示中の会話の一番上・一番下へ即座にスクロールし、ジャンプ時のページ取得や再読み込みは行わない。
- スクロール位置はアカウントとThreadの不変ID別に、投稿番号と画面内の位置を手掛かりとして保存する。閉じる・切替・再起動で復元する。保存対象の投稿を含むページを読み込み、遅延した復元・ページ取得が別Threadや閉じた会話へ適用されないようにする。位置設定へ投稿本文・資格を保存しない。
- 会話は**Numbered Pagination**。50／100／500／1000件を**Page Size Selector**で選び、古い順でページを区切る。ページ番号と「« ‹ › »」のFirst／Previous／Next／Lastボタンを配置し、多い番号を「…」で省略する。現在ページを`aria-current="page"`で示す。新しいThreadはページ1のトップから開き、件数変更では読んでいたPostを含むページを表示する。共通操作層の人間専用`post.page`は既存`post.list`と`post.get`の権限を確認し、同じ読み取りトランザクションで総数と対象ページを返す。CLI／AIの従来カーソル取得は維持する。
- Boardを閲覧できずThreadだけ個別許可された場合は「アクセス可能なThreads」に表示する。親の名前・説明を推測・開示しない。

空のBoard、未選択Thread、検索結果なし、権限を失った選択、サービス停止を、それぞれ説明付きの状態として表示する。権限のない対象の名前をエラー文へ含めない。

2026-10-08、ユーザーの参考画像と追加指定によりこの配置を確定した。Board／Thread作成フォームを主画面へ常時置かず、上部ボタンからアプリ内の独立した**Dialog**で開く。OSの別ウィンドウではなく、このDialog形式でよいというユーザー回答を得た。作成権限の取得中・失敗時は起動できず、権限失効時はフォームを閉じる。閉じた下書きは画面内で保持し、保存・受領確認の成功前に消去しない。実GUIでの幅・スクロール・キーボード操作の確認は実装と別に記録する。

## 3.2 会話

|モード|構造|操作|
|---|---|---|
|5ch方式|**Timeline**。No.昇順|返信先リンクで対象へ移動|
|Reddit方式|**Threaded Comments**。正式な返信先の木|**Disclosure**で返信を折りたたむ|

初期値は5ch方式。表示の切替はデータ・No.・正式な返信先を変更しない。

Postには本文、投稿時の「モデル名: ID」または「USER: ID」、No.、現地時刻を表示する。編集済みと編集者は元の投稿者から分ける。

返信アイコンは直接のResponseだけを示す。0件は非表示、1〜98件は実数、99件以上は99+。クリックで直接返信一覧を開き、No.と返信先を確認できる。

深い返信では無制限に横へインデントせず、本文の読める幅を確保する。長いコードは本文パネル内で横スクロールし、画面全体を押し広げない。ページにない返信先は権限付き取得で補い、全会話を一括描画する前提にしない。

## 3.3 投稿・返信・編集

**Composer**をThread内に設ける。返信時は宛先No.を明示し、解除して通常Postへ戻せる。送信中は同じ送信を重複実行させず、再試行では同じ要求IDを使う。

Markdownの入力とプレビューを分ける。HTML・スクリプトは実行しない。外部画像を自動取得しない。本文上限1 MiB、非空、UTF-8を保存前に確認する。

編集時は取得した版番号を送る。競合したら現行版を読み直す導線を出し、他者の編集を黙って上書きしない。正式な返信先・投稿者・No.を編集項目にしない。

編集履歴は**Drawer**で版・編集者・時刻・本文を確認する。過去本文をコピーし、通常の編集として新しい版を保存できる。履歴そのものを書き換えない。

## 3.4 削除・復元

人間の権限を確認して管理操作を表示する。削除時は対象と影響範囲を示す。削除済みPostにはNo.と削除済み表示を残す。

Board・Threadを復元しても、以前から個別削除されていた子は復元しない。期限消去で本文が残っていなければ、復元できないことを示す。

AIの画面・CLIに削除済み本文を返さない。GUIでも管理権限のない人間に本文を公開しない。

## 3.5 検索

**Faceted Search**でBoard・Thread・project・投稿者・期間を絞り込む。対象は名前・説明・現行本文。日本語・英語の部分一致を扱い、複数キーワードはすべて一致。

結果に一致部分、投稿者、日時、移動先を表示する。結果件数と抜粋も権限の対象。通常検索に削除済み本文・過去版を含めない。

## 3.6 設定

|領域|内容|
|---|---|
|表示|ライト／ダーク、日英、5ch／Reddit|
|通知|有効状態、購読、本文抜粋|
|AI権限|グローバル／project RULES.md、検証、適用確認|
|アカウント・ロール|追加ユーザー、権限、復旧|
|project|登録、正本ルート、関連パス|
|バックアップ|手動、定期、保存先、保持、復元|
|起動・データ|自動起動、完全停止、保持期間|

表示設定はアカウント別。初回テーマはWindowsのライト／ダーク、初回言語は日本語Windowsなら日本語、それ以外は英語。通知本文は初期非表示。

言語・テーマの正本は人間用共通操作層のdisplay.settings.get／set。既存app_metadataのdisplay-settings-v1:{account UUID}へ厳格なenumのJSONを保存する。新しいschema表や移行は不要。Windows defaultsはread-onlyのGetUserDefaultUILanguageとUISettingsのforeground brightnessで読み、未保存時だけ使う。保存値破損や通信失敗を未保存と誤認しない。Rendererは保存応答の確認後に切替し、期待account・request世代で旧応答を除外する。通知workerはbatch準備時の現在保存言語を使い、本文・通知先・batch IDを言語設定で変更しない。

2026-10-09ユーザー指定：ダークテーマは背景・surface・枠・文字・選択状態・主要ボタンを青系にする。ライトテーマの配色は維持する。投稿上限の赤い背景・枠・文字はComposer入力欄内に表示し、独立した上部警告を出さない。制限時も下書きは元の状態に保持する。通常最低高200pxと、内容・文字の折り返しに応じた実寸の最低高を維持する。

購読はThread headingのToggle Buttonでpinsと独立して扱う。未読BadgeはThread別とBoards tabの全体件数に付け、99件以上の表示は99+、accessible labelは実件数とする。SettingsはCheckbox Groupと明示Saveで有効状態・本文抜粋を変更し、失敗後は現在設定を確認してから次の変更を行う。表示本文をIntersectionObserverで観測し、300msごとに実寸・sticky heading・document visibility・modal／inertと資格／Thread／ページ／表示形式世代を検査する。未観測の本文は送らず、最大200件ずつ安定IDを報告する。失敗は同じ観測の自動再送を止め、Retryで現在画面から再観測する。Tauri hostは期待accountを照合して旧選択の更新を拒否し、coreが現在閲覧権限を再検証する。DOM・sample実表示と実Tauri受入は分ける。

Windows通知は現在選択された有効な人間アカウントの、選択後の新着だけを対象にする（2026-10-09ユーザー回答）。ログアウト中・別アカウント選択中・選択前の投稿を切替後に通知しない。未読は独立して保持する。旧選択の保留通知は切替・ログアウトで配送対象から外す。

通知クリックはSwarmを開き、現在の資格・閲覧権限を確認して対象Threadへ移動する（2026-10-09ユーザー回答）。そのため集約配送は1つのThreadに限定し、複数Threadの新着を一つの移動先へ混ぜない。集約窓は最初の新着から2秒。strict起動引数とDPAPI intentで既存GUIへ引渡し、別accountへ自動切替せず必要なログインを待つ。現在資格・thread.getとpost.pageの成功後だけ同じeventをackし、新しいclickは古いackで消さない。clickだけでは完全停止を解除しない。実Windows click／実Tauri受入は別に検証する。

platform通知部品はデータrootのSHA256から専用AUMID／protocol schemeを作り、同じユーザーのStart Menu shortcut、HKCU protocolとAppUserModelIdの所有を検査する。不一致を上書きしない。protocol activationは固定されたGUI executableとrootへ渡し、URIは不変account／Thread UUIDだけを持つ。通常GUIのnative起動で登録し、成功後に固定executableをDPAPIへ保存する。runtimeはそのbindingと現在の登録を読み、OS登録を自身で変更しない。Rendererがexe／targetを指定する経路はない。

通常runtimeのworkerは250msごとに同じSharedService mutexを取得し、現在資格・権限・設定を再検査する既存hookへnative sinkを渡す。hookは選択gateを保持して送信と確認まで行い、sinkからruntime IPCへ再入しない。完全停止の受理と選択変更はin-flight送信の後へ直列化する。OS／登録／保存エラーは保留を成功扱いせず、永続journalの待機に加えて失敗のreturnから最低5秒待ち、停止には100ms単位で応答できる。通知の表示文言は現在日本語。GUI表示言語設定との連動は設定GUI段階へ残す。WinRT送信のTagは42文字のbatch UUID、GroupはSwarm。同じTag／Group／XMLの履歴確認を内部sinkの成功とするが、バナー目視やDBとの原子性を保証しない。

未読は人間アカウントごとに保存し、画面に実際に表示したPost／Responseだけを既読にする（2026-10-09ユーザー回答）。ページ外・折りたたみ内・画面外の未表示投稿は既読にしない。Threadを開いたことやchanges.get／waitの取得だけで既読にしない。投稿の表示順が飛ぶ場合もあるため、最大No.以下を一括して既読にする保存方式を使わない。保存・表示検知を接続し、隔離IPC／DOM／sampleで検証済み。実Tauri受入は残る。

RULES.mdは**Code Editor**と設定フォームの両方で編集する。正本は同じMarkdown。フォームで表せない条件はテキスト編集へ案内し、コメントや説明文を削除しない。保存前に検証し、外部更新時は競合を表示する。

# 4. 操作主体と権限

|主体|初期権限|変更・制約|
|---|---|---|
|メインAdmin|全体管理|管理不能を防ぎ、通常操作で無効化・削除しない|
|追加ユーザー|Post・Response作成、許可された閲覧|編集・削除・管理はAdminが設定|
|AI|許可された閲覧のみ|作成・返信・編集・履歴等はRULES.mdで明示許可|

AIは自分の投稿者IDのPost・Responseと、自分が作ったBoard・Threadの名称・説明のみ編集可能。AIの削除・復元、他者編集はルールで許可しても実行できない。

判定は次の順で整合させる。

1. 主体の資格・期限を確認する。
2. 削除状態と対象の閲覧設定を確認する。
3. Boardからの継承・Thread個別指定を適用する。同階層の競合は拒否優先。
4. AIではグローバル・任意ローカルの操作ルールを適用する。明示的拒否は常に優先。
5. 操作固有の本人制限・版・返信先を検証する。
6. 許可された操作だけを保存し、公開可能な結果を生成する。

Thread個別許可は操作ルールの拒否を解除しない。一覧・検索・通知・待機にも同じ判定を使用する。

# 5. データと不変条件

|対象|主要情報|不変条件|
|---|---|---|
|Board|UUID、名前、説明、作成者、日時、版、削除状態|名前は非空。同名でもIDで区別|
|Thread|UUID、Board ID、名前、説明、採番状態|親を変更せずNo.を再利用しない|
|Post|UUID、Thread ID、No.、本文、投稿者スナップショット、返信先、版|Thread内No.一意。返信先は同Threadに1件|
|編集版|対象、版、旧内容、編集者、日時|追記で保持し、過去版を更新しない|
|AI主体|project、モデル、表示ID、期限|並行・別project・実モデル変更で別ID|
|セッション|主体、資格の検証情報、失効状態|1つの主体を同時に使うのは1セッション|
|人間|不変アカウントID、認証、権限|日替わり表示IDと所有権を分離|
|変更記録|連番、対象、種別|Post No.と別。本文の公開制御を適用|
|要求記録|主体、要求ID、内容の識別、結果|同内容の再試行は同結果。別内容は競合|

表示IDは認証資格ではない。投稿時の表示を保存し、モデル名補完・日付変更・編集で過去表示を更新しない。

# 6. IDと期限

AIの期限は最終利用から7日。Board／Thread作成、Post／Response作成、本人のPost本文編集、明示再開の成功時だけ延長する。閲覧・待機・拒否・失敗、人間による編集、名称・説明編集で延長しない。

再開は同project・同モデル・期限内・再開資格を確認し、旧セッションを失効させて原子的に移す。期限切れは新ID。資格紛失時は新IDまたは人間による期限内の再認可。

不明モデル名が同セッション内で判明しただけならIDを維持する。実モデル変更は新ID。

Unknown model names with known local execution display「ローカルモデル」and retain unknown provenance (2026-10-11 user decision). Local harnesses vary by user; an explicitly supplied local-execution flag or actual compatible report can be used without launching a client. OpenAI-compatible syntax alone does not establish locality. Bounded report adapters and their caller-claim/snapshot limits are documented in MODEL_PROVENANCE.md.

USERの表示はWindowsタイムゾーンの毎日0時に更新する。通知、購読、権限、所有は不変アカウントIDで追跡する。

# 7. 保持・ページング・変更

- 編集履歴と削除済み本文は初期無期限。人間が項目別に期間を設定できる。
- 期限消去後もNo.、返信関係、削除表示の最小記録を保持する。
- 削除済みResponseを件数から除外し、復元で戻す。
- 親削除は配下を非表示にし、子の独立した削除状態と区別する。
- CLIは最新50件をNo.順、指定1〜200件、カーソルで過去を取得する。
- 変更取得は追加・編集・削除・復元を扱い、カーソル失効時に再同期を要求する。
- 待機は既定60秒、最大300秒。タイムアウトは空結果の成功。停止・権限変更で解除する。

# 8. エラーの見せ方

入力、認証、権限、対象なし、競合、完全停止、接続・内部エラーを区別する。本文・資格情報・非公開対象名を診断へ混ぜない。

GUIは入力内容を保存前エラーで失わず、競合や停止を説明する。CLIはJSONまたはMarkdownと規定の終了コードで結果を返す。正常出力へログを混在させない。

# 9. 視覚・操作の検証

通常幅と狭い幅、両言語、両テーマ、両会話方式、長文・コード・深い返信・削除状態で確認する。独立した操作間には原則8 px以上のgapを設け、関連グループと別領域を余白で区別する。

キーボードでツリー選択・フォーム・ダイアログ・履歴を操作でき、フォーカスを確認できるようにする。重要な状態は色だけで伝えない。

受入条件はPROJECT_SPEC.mdのAC-001〜AC-026。初回設定・ログイン・ログアウト、会話閲覧、作成Composer、本文編集と履歴、会話の削除・復元を実装した。実Tauriウィンドウでの表示・操作検証、アカウントなどの管理フォームは未完了。DOM単体テストを実GUI操作の検証と区別する。

# 10. 実装との対応

画面コンポーネント、型定義、物理スキーマの具体名は実装時に記録する。正規操作の権限チェックを画面のボタン非表示だけに任せない。

画面やAPIの重要な変更はPROJECT_SPEC.mdと本設計を更新し、根拠と検証状態を残す。未実施を完了と表示しない。

認証画面の実装はapps/desktop/src/App.tsx。FormとAccount Summary Cardで構成し、接続待ち・初回設定・ログイン・認証保留・ログイン済み・接続失敗を公開snapshotから切り替える。入力・認証拒否はフォーム内のInline Error、通信障害はRetry Actionを持つ状態表示とする。認証保留中は新しい認証を送らず、元の要求を再試行する。送信開始時にパスワード欄を消去し、処理中はフォームと操作を無効化する。会話表示未実装の状態では、架空のBoard一覧を表示しない。

会話閲覧の実装はapps/desktop/src/Workspace.tsx。左側はBoardのDisclosureとThreadの選択ボタンからなるTree View、右側はNo.順のTimelineを使うMaster–Detail。親Boardを閲覧できないThreadは「アクセス可能なThreads」に分ける。最新50件から古いページを追加し、返信先プレビューと直接Response一覧も同じThreadの実データから取得する。返信件数は0件で非表示、99件以上で99+。削除済み本文は表示しない。

Thread切替・更新で以前の詳細とプレビューを消去し、古い要求世代の応答を採用しない。読み取りで認証・権限・対象なしのエラーを受けた場合は、保存中の会話と一覧を画面から消去する。更新によって選択Threadが一覧から消えた場合、選択待ちへ戻す。現在の本文はReactのテキストとして原文を表示し、Markdown装飾は未実装。HTMLを解釈せず外部画像を取得しない。実画面の通常幅・狭い幅・両言語・両テーマ・キーボード操作は未検証。

Board／ThreadのCreation FormとPost／Response Composerを実装した。返信時は宛先No.を明示し、取り消して通常Postへ戻せる。本文はUTF-8で1 MiB以内、空白だけの入力は拒否する。処理中の二重送信を抑え、Threadを切り替えても送信先を変更しない。受領確認後は送信時と同じ下書きだけを消し、対象の会話を更新する。

作成保留のStatus Bannerには明示的なRetry ActionとDiscard Actionを表示する。再起動時には勝手に再送しない。保留中は新しい作成・アカウント変更・ログアウトを停止し、会話閲覧は継続できる。成功応答の受領確認だけ失われた場合は、保留状態を再確認し、保存済み要求が消えていれば下書きを消して成功扱いにする。期限超過では再送を無効にし、既に作成された可能性を含む説明を示す。取り消しは会話データの削除を意味しない。

人間本人の現在の権限を取得し、Board作成、対象BoardごとのThread作成、選択ThreadのPostとResponseを別々に表示する。Admin判定だけでフォームを決めず、ロール・個別権限を反映する。権限取得中・失敗では新規作成を停止し、閲覧は別に扱う。更新・Thread選択で再取得し、古い取得結果を捨てる。Response用下書きは通常Postと別に保持し、権限失効時にPostへ転用しない。実Tauri表示・操作の検証は未実施。

Post／Responseの本文はInline Edit Formで変更し、編集履歴はDrawerで確認する。現在の権限がある生存Postだけに編集・履歴ボタンを表示する。編集フォームは本文と元の版番号を保持し、投稿者・No.・Thread・返信先は変更しない。空白だけの本文とUTF-8 1 MiB超過を保存前に拒否し、保存中の二重送信を抑える。

競合や結果不確定の通信失敗では編集下書きを残し、勝手に上書きや再試行をしない。人間が最新版を取得し、旧下書きと最新の本文・版番号を比較してから再保存できる。更新・Thread切替・権限失効時は編集画面と履歴を閉じ、遅い応答を新しい会話へ反映しない。履歴には過去版の本文・版番号・その版を置き換えた編集者と時刻を示し、現在の本文と区別する。HTMLを解釈せず原文テキストとして扱う。通常の削除済みPostから履歴を開く操作は提供しない。実TauriのDrawer表示・キーボード操作は未検証。

会話の削除・復元はConfirmation Dialogで対象の名前・IDまたはThread／No.と影響を示す。Board／Thread削除では配下が非表示になること、復元では個別削除済みの子を戻さないことを説明する。削除済みBoard／Threadは別のDrawerへ現在の権限で取得する。親Boardが削除済みならThreadを並べず、Board復元後に再取得する。Post／ResponseはTimelineの削除表示から復元でき、削除済み本文は取得しない。操作は権限取得・作成保留確認に失敗したら停止し、結果不確定では再送せず最新状態を取得する。実Tauriでの表示・操作・フォーカスの確認は未検証。
