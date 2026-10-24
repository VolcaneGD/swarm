# 実アプリ操作が必要な残テスト

2026-10-24 21:46JST：初回βのGoogle操作非提供をGUI／core・hostで実装し、配布runtimeの拒否・通常password／復旧code／backup設定と正常終了を確認。UIは日英・狭幅のliveブラウザーpreviewでありnative入力受入ではない。最小M1–M5技術blocker0・公開指示待ち。正式363/17/380／PASS0・全native正式条件／明示保留は保持。最終候補証拠 (maintainer-only record omitted from this review export)。

2026-10-24 21:07JST：最短βの必須範囲は[BETA_RELEASE_GATE M1–M5](BETA_RELEASE_GATE.md)。1件の実RULES保存中断artifactをfresh配布runtimeで読取・書込拒否→正規API復旧し元policy／Post保持・正常終了を確認。native GUI／全OS電源断は受入にしない。通常失敗出力の実canaryは再利用し未再現例外の網羅探索を追加検証へ分離。17行は正式／保留／公開後の全体残数で、公開前の残はGoogle提供範囲・最終候補の2点。

2026-10-24 14:38JST：N10の固定配布runtime実password／公開session開始・再開をSQL未確定→停止frame読取→commit／正常停止へ結合。新旧資格・同account／actor・release／同Post不変を確認。native入力はなし、全business mutation／host／OS受入は維持。363/17/380・正式PASS0。結果 (maintainer-only record omitted from this review export)。

2026-10-24 13:41JST：N10の固定配布runtimeで待機接続107opcodeの停止後RUNTIME_STOPPEDを確認。private GuiClientの認証済停止APIでありnative GUI操作ではない。引数検証前拒否／同Post1件保持／7batch・最終readback正常exit0／起動OFF・登録なし。全有効更新RPC・開始済transaction／native・OSは未完、362/17/379・正式PASS0。結果 (maintainer-only record omitted from this review export)。

2026-10-24 13:30JST：固定archived実日数のprivate／同公開AI readsetを確認し、Thread・一覧・代表Post・履歴NOT_FOUND／生存対照不変／正常release・exit0。API subsetのみ完了、native GUIは未観測。通知利用不可警告を配送受入に使わない。361/17/378・正式PASS0、N10全RPC／O03残契約は維持。結果 (maintainer-only record omitted from this review export)。

2026-10-24 09:14JST：修正版の実VM subsetを確認。起動識別修正版のsameboot停止／CLI7、Run修正版のnative ON／OFF・GUI終了後runtime継続・有効再起動の背景runtime1／GUI0・無効再起動の停止／CLI7、同一ユーザー昇格作成→通常権限で同じ5lease取得、原22Data/PATH・登録解除／通常Off。N10全更新RPC drain、O03初期有効等の全契約は未完。sameboot証拠はRun修正前候補であり、修正後の実dispatch同bootを再観測したとはしない。実受入1行分離で360/17/377、元残8完了／17未完・正式PASS0。証拠 (maintainer-only record omitted from this review export)。

2026-10-23 13:28JST：非GUIのR07実purge後明示history grantを完了し、新過去Thread1000件の実1日baselineも準備済み。次の実観測は2026-10-24 13:23:41JST以後。通常release／stop受理・process不在、初回補助timeoutと同Data未作成suffixの回復を保持。native GUI／実経過purgeは未受入、既完了caseを反復しない。359/17/376・正式PASS0。grant実結果 (maintainer-only record omitted from this review export)・新baseline (maintainer-only record omitted from this review export)。

2026-10-23 08:12追記：有効accountの資格resetと待機実OS非配送／fresh再認証後新着のみを確認。
通常APIと実OS履歴の証拠でありnative GUI入力／banner／clickは未受入。4Post保持・正常終了／登録解除。
358/17/375・正式PASS0、完了case反復禁止。証拠 (maintainer-only record omitted from this review export)。

2026-10-23 02:21追記：R07購読なし直接返信の待機target削除を固定runtime／実OS履歴へ限定結合。
元34＋新7Post保持／通常終了・登録解除。positive再投稿なし、既存履歴Content障害に対するTag baseline
観測制約を保持し、native GUI／banner／click／全履歴健全性の代用にしない。357/17/374・正式PASS0。
実結果・2失敗と回復 (maintainer-only record omitted from this review export)。

2026-10-22 23:51追記：R07の待機中Thread／親Board read拒否・拒否role割当と個別Post削除を
固定runtime／通常API／実OS履歴で部分確認。native GUI／banner／clickの完了には転用しない。
最終34Post保持・全4runtime通常終了／登録解除・8stream0。356/17/373・正式PASS0。
権限3join (maintainer-only record omitted from this review export)／個別Post削除 (maintainer-only record omitted from this review export)。

2026-10-22 23:06追記：同一native失敗／rollbackへの実発行human2／AI token・resume／cursor結合を確認。
旧資格全拒否、手動native再login後旧cursor3件拒否・fresh保持、元2Post／RULES不変とnative表示、
正常終了／登録解除。旧live broker・他fault／OSは未受入。全phase消費済み、354/17/371・正式PASS0。
証拠 (maintainer-only record omitted from this review export)。

2026-10-22 22:16追記：R18既復旧post-active failure Dataで人間がnative再ログインを完了。
Skyで元2投稿を表示、GUI通常終了後同account／rolled_back／ID・body hash・revision・No／RULES照合。
復元／fault反復なし、通常stop／登録解除・4stream0。限定subset完了、全N13／資格・OS行列は未受入。
証拠 (maintainer-only record omitted from this review export)。353checked/17open/370total・正式PASS0。

2026-10-22 09:32JST：可変高trialはlazy初回506MiBだが検索／長文等の操作区間581MiBで512MiB未達。検索No53／返信No52／collapse＋Space成功を全N14へ転用せず試作撤回。通常描画復帰138tests／型／release成功、全Data保持／正常終了。通知snapshot抑制は保持。採否と限界 (maintainer-only record omitted from this review export)。

2026-10-22 08:47JST：日時formatter共有・全404tests／型検査／release成功。同じ1000件native検索No53／深度16／隣長文・No1000表示確認。open peak739340288bytesで512MiB未達、後の長文512708608bytesだけでN14を閉じない。全Data保持・正常終了／登録解除。証拠 (maintainer-only record omitted from this review export)。

2026-10-21 02:34JST：N03／N07の現Thread拒否→実Windows通知センター配送抑止を部分確認。直前承認の購読／別author positive1実entryと専用historyを対照に、拒否後1件の旧alias拒否・entry不増・revoked marker0／readonly outbox0を確認。応答不可モード有効のままでbanner／clickは未受入。通常API stop／所有登録解除と人間GUI通常終了・独立両PID退出、350/19/369。全N03／N07／N11／正式ACは残す。限定証拠 (maintainer-only record omitted from this review export)。

2026-10-21 02:13JST：N07実経過の公開AI追加readset／通常releaseに成功し、前段native／private subsetへ接続。native完全停止中表示→GUI通常終了、両process退出・専用通知登録解除／baseline保持を確認。準備済み1日ケース部分は分離完了、349/19/368。未準備archived Thread、履歴明示grant、残る権限・changes・通知・全N07／正式ACは未受入。実結果 (maintainer-only record omitted from this review export)。

2026-10-21 01:58JST：N07実経過観測1回を明示許可で実施。native管理者／4種1日設定、生存No1本文、No2期限切れ復元不可・削除Board／Thread一覧空を確認。private読取／復元拒否を通過後、公開CLI応答判定で失敗・位置不明。公開AI全件／archived Thread／通知・全N07は未受入、追加枠0で再実行なし。AI通常release・native完全停止／GUI終了・所有登録解除／baseline不変を確認、348/19/367維持。証拠 (maintainer-only record omitted from this review export)。

2026-10-20 18:53JST：実elevated guestの新規ciphertext／backup／lockを含む5対象を別PowerShellで2holder計10観測、本人owner／本人1ACE／非継承、競合待機→解放→再取得・旧Data22file保持に成功。既存認証済みhuman正常停止を別version証拠として明示結合しROADMAP R01を完了、348/19/367・元25は6closed/19open。最終通常Off／元chain／baseline／切断networkを照合。native全条件や正式ACのPASS追加なし、完了行列を再実行しない。記録 (maintainer-only record omitted from this review export)。

2026-10-20 18:31JST：固定VMの保持済み候補で稼働中GuardRunning23→認証済み停止／両stream回収→idle0、通常NSIS self-copy uninstall／stable3、元Data22file全hash保持・PATH値型復帰・所有登録解除PASS。正常Off／元chain／baseline／切断networkを照合。これはOS／非GUIの実検査で、通常画面／削除checkbox／通知登録・独立ACLは未確認。O02全体・正式ACのPASS追加なし。ROADMAP部分完了1行で347checked/20open/367total。Installed削除済み、検査を再実行しない。記録 (maintainer-only record omitted from this review export)。以下の未投入／稼働中拒否未実行は履歴。

2026-10-20 12:49JST：N07の削除済み親子の選択制限／取消し、承認済み内容Board単体purgeと配下Post／返信／履歴の11経路NOT_FOUND・非復活、別fixtureの親＋独立削除子一括復元を実nativeで確認。両子の本文／返信と子2履歴を実描画、製品APIで同ID／revision／本文／履歴数と元Post不変を照合。単体／一括purge後のWorkspace保持も確認済み。346/20/366を維持し全N07／正式ACは未受入、保持期間・通知・public AI等の残条件を保持。確認待ちなし。記録 (maintainer-only record omitted from this review export)。下の人間結果待ちは履歴。

2026-10-18 H1進行中：ユーザーが実nativeの「スキップ」でパスワード設定省略、コード表示／非表示、誤アカウント切替拒否／取消し、Member追加を実施。実native／ユーザー画像でログイン済みAdmin・ロック無効・Member有効を照合。通常submitの空欄拒否は仕様通り、親の案内を訂正。部分条件をROADMAPへ分離、全N01／N02等は未完了。現在はMember管理の取消し／無効化・有効化／resetと旧・新パスワード切替の人間検査結果待ち。記録 (maintainer-only record omitted from this review export)。β先行対象はCodex＋Gemini CLI／WebView2導入済みWindows11 x64に決定、他の安全性条件・正式ACは維持。

2026-10-16 22:27追記：330checked/25open/355total。実候補の子processだけのWebView2診断フラグでDevTools／rendererログ観測経路を確立し、通常投稿閲覧と5秘密canary／4出力の露出0を確認。合成例外はbrowser貼付け安全guardで拒否され、未実行／未解除のためN04例外受入は残す。別のruntime実時間回帰で、通知失敗後のPost削除・Thread／Board完全削除とworker／Service再openを3経路確認し、生存比較Postだけを再送。これはnative通知／実日数保持の代用ではない。GUI通常終了・専用runtime停止・当該登録解除、本番52064保持。renderer (maintainer-only record omitted from this review export)・通知 (maintainer-only record omitted from this review export)。

2026-10-16 22:02追記：N03の検索単独拒否／response.list単独拒否を実nativeで確認。通常投稿読取は継続許可、旧capabilityは不変で旧検索カード／直接返信関係パネルが残留。private capabilityへsearch／response_readを追加したmatching候補で各消去を確認。同一Data／資格、投稿と返信関係不変、全core259／frontend395／private pipe1と型・fmt・Clippy成功。検査GUI／runtime／登録片付け、本番52064保持。全N02／N03等・正式ACは未完了のまま327/25/352。証拠 (maintainer-only record omitted from this review export)。

2026-10-16 20:46追記：N02／N04の資格再設定・通常パスワード変更・Admin復旧3経路を専用Dataで一括確認。旧実GUI本文／履歴消去→サインイン、旧資格・旧パスワード・使用済み復旧コード拒否、新資格と他ユーザー資格保持、15秘密canary／8出力＋6native surface漏出なし。native認証フォーム入力は未実施。N03／N07の削除済み一覧だけの権限取消し残留を旧実native再現→修正matching候補で消去確認。全391test＋最終単体／一括4focusedと型成功。検査プロセス／登録片付け、本番52064保持。326/25/351、全native群／正式ACの他条件は維持。資格 (maintainer-only record omitted from this review export)・修正 (maintainer-only record omitted from this review export)。

2026-10-16 19:53追記：N02／N03のロール取り消しを3表示経路で追加確認。親Board非表示・Thread個別role許可、同じ旧資格の取得拒否、実native設定一覧／Activity／ピン留め・本文の消去を確認。設定経路は件数・未読0、Activity／pin経路は私有ペイン消去、再付与後の更新／Retryで復帰。保存Post不変、検査GUI／runtime／登録片付け済み。本番52064保持、全N02／N03・native権限編集・通知配送と正式26ACは未完了／NOT_RUN。325/25/350。証拠 (maintainer-only record omitted from this review export)。

2026-10-16 15:58追記：N04の実process stdout／stderrと実競合エラー／公開ログを観測し、5秘密canary／8出力file＋4native surfaceで漏出なし。N13正常適用から実発行済み人間2session／AI token・resume失効、手動再ログイン後の保存投稿一致と旧human cursor拒否までを確認。失敗誘発2試行は予約解放後に正常適用、rollbackは未観測。全N04／N13のチェックは閉じない。検査GUI／runtime／登録を片付け、本番52064保持。324checked/25open/349total、全26formalAC NOT_RUN。出力 (maintainer-only record omitted from this review export)・復元 (maintainer-only record omitted from this review export)。

2026-10-16午後追記：新しいmatching候補・専用Dataで、N07追加機能の単体完全削除／全選択3件の完全削除・取消し、選択した親子の一括復元、非選択兄弟保持を実native確認。4Threadの削除後本文取得拒否と比較Post不変を照合。保持3日数の保存・再取得と無期限へ復帰、安全コピー284.6KiB→0B、手動／定期backupの個別削除・実ファイル／receipt整理も確認。backupパス別名の重複／pending保護不一致を発見・修正し、修正候補で再確認。322checked/25open/347total。N07の実期限経過・本文非復活／通知経路と全N13／正式ACは残す。証拠 (maintainer-only record omitted from this review export)。

2026-10-16 07:36追加機能：単体／選択／全選択の復元・完全削除、削除済みBoard／Thread／過去スレ日数、backup／復元artifact整理を実装し、backend／実private pipe／DOM／実ブラウザ・全Rust565件／新規36file候補を確認。321checked/25open/346total。native不可逆操作等は下のN07へ明示追加し、今回のソフトウェア検査をnative受入に置換しない。全正式AC NOT_RUN、公開・OS再試行上限は維持。証拠 (maintainer-only record omitted from this review export)。

2026-10-16 03:11追記：N07の残native境界を前進。専用実Tauriで返信0非表示／98実数／99・100が99+、99件全読込、公開CLIの正確な実件数を確認。Board／Threadの取消し・実行時承認後削除／復元、削除中AI本文拒否と同一ID／本文への復帰、個別削除済み子の保持を確認し、ROADMAPの既存確認／一覧／親子状態行を完了。N07の保持期限後の非復活・通知経路は未完了のため下の全体チェックは残す。317checked/25open/342total、全正式AC NOT_RUN。証拠 (maintainer-only record omitted from this review export)。

2026-10-16 02:52追記：N02／N03／N04の設定cache漏れを修正。旧候補の実Member GUIでaccount.list取消し／Service exit4後もAdmin一覧残留を確認し、新matching候補では同じ取消し後に設定モーダル／Admin名消去を確認。全frontend378tests、型、staticCRT／NSIS／35file候補成功。各role／RULES操作の全native行列は未受入、ユーザー報告のN05手動試験は未実施。旧／新専用GUI・runtime・通知登録片付け完了、本番52064保持。315checked/26open/341total、全正式AC NOT_RUN。証拠 (maintainer-only record omitted from this review export)。

2026-10-16 02:37追記：N04の実ログ展開・初期読込／空Thread選択／更新／ログ内scrollを確認し、4実資格canary／4観測surfaceの露出なし。renderer console／例外経路とN04全体は未完了。N13の成功／DB書換え後rollback／資格消去後rollbackに対する実発行資格・cursor失効はNamed Pipe回帰で追加確認し、実native受入とは分離。N05はGUI閉鎖・保存baseline不変、手動検証結果なし。専用GUI／runtime／通知登録は片付け済み。314checked/26open/340total、N06／N08／N09以外の22native群・正式26ACは未完了／NOT_RUN。資格 (maintainer-only record omitted from this review export)・ログ (maintainer-only record omitted from this review export)。

2026-10-15 22:24追記：必須範囲のうち、別Threadを含む検索結果・履歴のみの権限失効時の消去を修正版実GUIで確認。Thread未選択の検索結果にpost.getだけを拒否（metadata許可／post_read=false）、履歴表示中にpost.historyだけを拒否（post_read=true）の両経路で私有表示を消去し、保存Post／履歴の不変を確認。右寄せtoolbarも資産修正版GUIで表示。初回資産欠落候補はFAIL保持、共通ビルド排他／dist監視で再生成。専用GUI／runtime／通知登録は片付け、本番52064維持。310checked/26open/336total、全N02／N03／N04と正式26ACの未完了／NOT_RUNは維持。証拠 (maintainer-only record omitted from this review export)。

2026-10-15 21:55追記：β必須範囲を優先し、網羅的な後回し候補は[BETA_RELEASE_GATE](BETA_RELEASE_GATE.md)に従い保留。開いた本文が別セッションの権限拒否／資格失効後も残る不具合を再現・修正し、matching候補の実GUIで本文消去・認証画面復帰を確認。13観測出力／8秘密canaryの非露出は限定証拠であり、展開ログ内容・renderer debug consoleを含むN04全体やN02／N03は未完了。GUI／専用runtime／通知登録は片付け、本番52064・既存Data・VM再試行上限を維持。現在307checked/26open/333total、正式26ACはNOT_RUN。証拠 (maintainer-only record omitted from this review export)。

2026-10-15 17:10追記：N09の残条件をmatching候補・専用9投稿fixtureの実Tauriで完了。日本語light／英語dark、Timeline／Threaded Comments、双方／片側不明、既知model／harness、No.8の過去snapshot保持と同じ作者のNo.9補完済み表示を確認。N08／N09の2群完了・残23群は全体未受入。正式26ACはNOT_RUN維持。専用GUI／runtime／通知登録の片付け完了、元Data／production52064とVM再試行上限維持。現在305checked/26open/331total。証拠 (maintainer-only record omitted from this review export)。

2026-10-15 16:41追記：既存Installed保持の継続scriptへProcess handle／Refreshなし・認証停止後stream回収を統合し、PS5.1の8検査群と独立reviewを完了。VMへ未投入、同一incidentの再試行上限は消費済み維持。O02／管理者file・lock行列／N13／正式ACの合格追加なし。現在303checked/27open/330total。記録 (maintainer-only record omitted from this review export)。

2026-10-15 16:22追記：OS検査で管理者token新規storeの所有者不一致を確認・修正。通常全Rust回帰／Clippy／matching候補、管理者VMの合成資格／manualbackupと4store owner確認は成功。稼働中拒否・通常self-copy uninstallは検査補助の終了コード／ログ共有問題で未実行。独立管理者file／lock／ACE検査も残す。VM正常Off／disk切断、N13／O02全体・正式26ACは未完了、現在302checked/27open/329total。証拠 (maintainer-only record omitted from this review export)。以下は過去の記録。

2026-10-15追記：N13専用fixtureで実nativeのSafety保存・最終確認から戻る・通常GUI再起動保持・承認後の適用・再ログイン要求・人間の再認証・保存本文への復帰／追加Post不在を確認。入力一時不在でも適用済みreceiptと再適用不可を表示。read-only DB／RULES／製品archive検査も照合、専用GUI／runtime／通知登録は片付け済み。失敗時rollback・発行済みAI資格／cursor行列・長時間／OS障害等は残り、N13全体と正式26ACは未完了。現在301checked/26open/327total。証拠 (maintainer-only record omitted from this review export)（以下は過去の記録）。

2026-10-14 20:45ユーザー決定：今回のheader修正はpreview確認のみ。旧GUIの手動復元質問は回答済みで待ち状態を解除し、新headerのnative確認は今回未実施として残す。一般のnative ROADMAP再開承認と正式AC状態は変更しない。

2026-10-14 19:52追記：native認証済みworkspaceを562x1025へ実変更でき、通常のSizeメニューは右辺を選ぶRightの次にLeftで動かす必要があることを確認。N01認証matrix全受入ではない。ユーザー追加header修正は日英／両テーマ／6幅browserとmatchingWindows候補で検査済み、新candidate実GUIは旧検査GUI最小化・手動操作検出により復元待ち。N04資格canaryも未実施、stdout/stderrのみで合格にしない。N08のみfullPASS／他24nativeと26formal状態維持。記録 (maintainer-only record omitted from this review export)。以下は過去の記録。

2026-10-14 19:09追記：使い捨てVMで実候補35hash／CLI／自動PATH登録・所有解除・値型保持・二重実行／同版更新と実uninstaller engine完了後のPATH所有記録・HKCU解除／Data22file全hash保持を確認。通常Off。O02の通常画面／checkbox・稼働中拒否・通知・通常self-copy起動条件、O03再ログイン／再起動、O01WebView2なしの全条件は未達。公式_?=待機経路でありUI受入ではない。N08のみfullPASS、24native群／正式26ACの全状態は維持。記録 (maintainer-only record omitted from this review export)。以下は過去の記録。

2026-10-14 15:55追記：最新source候補の実TauriでN13の保存先folder初期表示／filter／既存manual archive選択・metadata／global・project復元先／再選択取消し／選択解除まで確認。Safety・適用・巻き戻しは専用copyで残す。N01枠drag／標準Sizeメニューでも幅1906維持、狭幅未検証。通常Data用runtime9956を検出、既存Installed更新せず別sourceGUI30944で確認し通常終了。fixture77472・startup／shortcut／protected state保持。記録 (maintainer-only record omitted from this review export)。fullnative／正式ACは未完了。

2026-10-14 12:08追記：候補内installer保持修正は10test成功。VMのPowerShell Direct／転送まで確認したが、最終試行は既存Installed／結果／defaultDataの複合guardで停止（該当分岐は未特定）。通常終了Off確認、既存ファイル／checkpoint保持。OSインストール／更新／PATH／アンインストールの受入は未実施。N13のfile pickerもfocus／cache不一致で選択・準備・apply未確認。記録 (maintainer-only record omitted from this review export)。

2026-10-14追記：500／1000件の画面外layout抑止試験で最下部／再開位置／キーボードの部分検査は成功したが、検索No53の表示位置ずれとGUI1,016,975,360 bytesの512MiB未達により不採用。試験変更を撤回。深さ16への移動・狭幅／言語／theme・性能全契約は未完了維持。手動サインイン完了後の通常更新で保存ログインは維持。記録 (maintainer-only record omitted from this review export)。旧候補／PIDは履歴。

更新: 2026-10-13 JST。正本は[ROADMAP](ROADMAP.md)、正式受入条件は[PROJECT_SPEC](../PROJECT_SPEC.md#18-受入基準テスト状態)、部分証拠は[ACCEPTANCE](ACCEPTANCE.md)。

2026-10-14 09:35追記：不変Markdown再解析抑止を実装・回帰検査しmatching installed版へ更新。N14の1000件表示15sample GUI1,174,564,864 bytesで512MiB未達、runtime14,102,528 bytesは目標内。メモリ原因の解決／N14／AC-026完了にはしない。旧installed hash／PIDは履歴。証拠 (maintainer-only record omitted from this review export)。

2026-10-14 09:26追記：N07承認済み親No.1だけのnative削除・復元完了、子No.2／No.3不変・本文検索0→1を各6公開CLI確認。保持／Board・Thread／98・99・100件／通知等は残す。N14並列8読取検索40件成功、native1000件表示でGUIメモリ512MiB目標超過。画像だけの再試験も1,299,447,808 bytes、原因未確定、N14／AC-026合格にはしない。50件／page20復帰済み。以下のNo.1承認待ち記述は履歴。証拠 (maintainer-only record omitted from this review export)。

2026-10-14 05:50一括検査：最新候補を隔離hostへ更新（35hash／Data／PATH保持）、N10完全停止・AI拒否7・明示GUI復帰／通常GUI終了後runtime・CLI継続を部分確認。N13実手動backupの製品inspect／破損・切詰め・末尾拒否4確認。N07親No.1確認Dialogは承認待ち・未実行。全transaction／OS／restore適用／新規既知snapshot等は残す。証拠 (maintainer-only record omitted from this review export)。

2026-10-14 05:31部分証拠：N09人間Post／Response・旧AI No.1000の双方不明表示、N13手動backup作成／結果確認済み／実ファイルhash／定期2世代保持・native picker起動／取消し確認。N09既知・片側不明・新規snapshot／補完後表示、N13file選択・適用等は残す。picker focus矛盾はbounded再観察後も残り、入力せず取消し。正式AC／full groupのチェックは変更しない。証拠 (maintainer-only record omitted from this review export)。

2026-10-14現行：ユーザーがComputer Useのnative作業を明示再開し、N08でhttps://example.com/への実ブラウザー遷移を手動確認した。N08のみPASS、残24群NOT_RUN、正式26ACはNOT_RUN。以前のnative保留記述は履歴であり、公開・配布の保留と除外client指定は維持する。認証／security／privacy操作と個別確認境界はComputer Use Skillへ従う。証拠 (maintainer-only record omitted from this review export)。

2026-10-14ユーザー決定：Antigravity／Cursor Agent・CLI／OpenClaw／OpenCodeによる開発側client試験は今後も実施しない。これらは保留ではなく対象外で、正常対応の判断は配布後の任意ユーザー報告へ委ねる。以下の実Swarm画面検査は実WindowsアプリのComputer Useまたは人間の手動操作であり、client除外とは別。その決定時点ではComputer Useの再開許可はなく、後の明示承認で再開した。

現行のComputer Use実アプリ操作はユーザーの明示再開により許可済み。認証・security・privacyは人間操作、削除は個別の実行時確認、危険OS検査は使い捨てVMに限定する。β公開・配布は「公開してください」という明示指示までは行わない。この文書の作成や部分証拠は正式受入完了を意味しない。全26正式ACは引き続きNOT_RUN。

2026-10-14部分証拠：N06作成／同ID返信／編集／人間履歴／公開CLI照合／GUI終了中runtime継続／再起動保存、N07承認対象No.2の削除確認／tombstone／本文非公開・検索0／直接返信0／No.3非再利用／同ID復元・検索1／直接返信1を実操作で確認。No.2復元済み、読取alias解放済み。N06競合・nativeとCLI交互write、N07保持期限／親子削除／98・99・100件／通知の全条件未完了のためチェックは残す。証拠 (maintainer-only record omitted from this review export)。

## 記録方法と準備

各項目には候補ビルドのハッシュ／版、OS・WebView2版、表示倍率、言語・テーマ、検査日時、匿名化したfixture名、手順、期待値、実際の結果、画像またはログ、PASS／FAIL／BLOCKEDを記録する。チェックは記載した残条件をすべて満たした後に付け、ROADMAPの対応行も同時に更新する。既存の部分証拠は再実行理由がなければ使い、全条件のPASSと混同しない。

開発環境への導入前に、既存データをバックアップし、導入先・データ領域・検査対象アカウントを明記する。権限・認証・Google連携の操作は人間が行う。本文・資格・パスワード・復旧コードを報告に含めない。破壊的なOS検査は日常の開発環境やβ利用者には依頼せず、使い捨てWindows環境で行う。既存の満杯Thread、RULES、USER／USER2設定、購読状態を検査の都合で変更しない。

## 開発環境で人間が行う検査

- [ ] N01 初回認証の残マトリクス（AC-019／AC-023）。通常幅・最小幅、日英・ライト／ダークで初回Admin設定、任意パスワード省略、コード表示、認証保留取り消しを確認する。過去の単一条件での手動初回設定は再検査対象にしない。
- [ ] N02 資格・アカウント管理（AC-019）。アカウント切替、ログアウト、Admin復旧、資格変更・失効・無効化・保留要求・取消しを操作し、古い画面や資格で操作が続けられないこと、下書きと案内の扱いを確認する。通常切替・再起動時のログイン保持は既存証拠を参照する。
- [ ] N03 ロール・個別権限（AC-013／AC-015／AC-019）。ロール所属、個別許可／拒否を変更し、開いた画面の閲覧・操作・一覧件数・未読・履歴・検索・通知が現在権限に従うことを確認する。親Board非表示で個別Threadのみ許可されたケースも含める。2026-10-18：実native編集入力とThread拒否／継承復帰の可視対照を既存旧画面失効へ結合しROADMAP R13を完了。全N03には通知等の別条件があるため未完了を維持。証拠 (maintainer-only record omitted from this review export)。
- [ ] N04 資格非公開（AC-019、ROADMAP第3章）。native GUIプロセス／rendererの公開出力・ログ・エラーを非公開の検査用canaryで調べ、資格や復旧コードが漏れないことを確認する。CLI／runtime／私有通信の既存証拠とは分離する。
- [x] N05 RULES編集（AC-013／AC-014）。トグル、ルール編集の枠、検証前の制限ガイダンス、保存成功／失敗トースト、不正下書き、競合、同時更新、反映中の操作拒否を確認する。エラー時の下書き保持と現在のRULESへの反映を照合する。2026-10-18新規H1の実native Text／Form・invalid／cancel／競合保持／再読込／busy disabled／成功toast・元file hash復帰で確認。証拠 (maintainer-only record omitted from this review export)。正式AC全体・O04は別。
- [x] N06 実アプリとCLIの共有状態（AC-002／AC-003／AC-004／AC-010）。新規fixtureでGUI／CLI交互の作成・返信・編集、双方のstale revision拒否とGUI下書き保持／最新確認、同じID／No.／revision／所有者／本文／履歴とGUI再起動後の一致を確認。指定No5の人間API削除後No6非再利用・同ID対象へのNo7返信も実GUI／CLIで照合。native削除確認・OS再起動は別gate。証拠 (maintainer-only record omitted from this review export)（2026-10-16）。
- [ ] N07 削除・復元・保持期間（AC-005／AC-011／AC-012）。確認ダイアログ、削除済み一覧、親子の独立削除、直接返信件数0／98／99／100と99+、保持期限で消去した本文の非復活、AI読取経路・通知への削除済み本文非公開を確認する。2026-10-16追加機能のnative受入には、単体完全削除・checkbox／すべて選択の一括復元／完全削除の対象と取消し、削除済みBoard／Thread／過去スレの日数・期限消去、backup／復元artifact整理も含む。実行時確認は具体的な隔離fixtureの直前画面で行い、backend／private pipe／ブラウザでの成功をnative不可逆操作の成功に置き換えない。
- [x] N08 リンク（AC-023、ROADMAP第5章）。実TauriのHTTPS確認表示／取消し／Chrome起動を確認し、正しいhttps://example.com/への遷移はユーザー手動確認。mailto／file／javascriptは操作できないテキストとして拒否。既存Markdown装飾証拠を維持、正式AC-023全体は別gate（2026-10-14、証拠 (maintainer-only record omitted from this review export)）。
- [x] N09 投稿者表示（AC-007）。`No.9999 ID: abcde (モデル名: ハーネス)`形式、モデル／ハーネスの各不明fallback、人間Post／Response、過去snapshotと補完後の新規snapshotを実Tauriで確認。2026-10-15 matching候補の専用9投稿fixtureで日本語light／英語dark・Timeline／Threaded Comments、No.8不明保持と同じ作者IDのNo.9補完済み表示を照合。旧No.1000／legacy部分証拠も維持。合成保存reportを用いた表示受入で、実client導入や正式AC-007全体ではない。記録 (maintainer-only record omitted from this review export)。
- [ ] N10 バックグラウンド動作（AC-018）。GUIを閉じたままCLIを使い、完全停止中の全更新RPCのdrain、復帰、同一boot内と次bootの停止条件を確認する。既存の通常終了／Start証拠で全契約をPASSにしない。
- [ ] N11 Windows通知（AC-020）。GUI稼働中／終了後のバナー目視、通知アイコンとWindows通知設定の表示、本文抜粋設定、購読、自己投稿抑止、権限変更、アカウント変更を確認する。通知clickで既存／終了済みGUIが起動・再認証し、許可された正しいThread／Postへ移動することを確認する。API配送成功だけで合格としない。
- [ ] N12 実Google連携（AC-019、ROADMAP第5章）。登録済みGoogleアカウントで連携・解除・復旧・拒否・応答喪失を確認する。プライバシーポリシー、OAuth branding／公開範囲の確認は公開前の別条件。署名fixtureを実受入と扱わない。
- [ ] N13 バックアップ／復元（AC-021）。nativeファイル選択、手動／予定バックアップ、適用、取消し、失敗時の巻き戻し、再認証、資格／cursor無効化、長時間gateと完全停止を専用コピーで確認する。元データは触らない。
- [ ] N14 native資源・性能（AC-026）。大量・並行workloadの目標を測定前に決め、CPU／RAM／版／データ量／負荷を記録し、GUI応答・検索・runtime／GUIメモリ・最大復元を測定する。過去の小規模baselineやdebug1GiB確認だけで完了にしない。

## 使い捨てWindows環境で人間が行う検査

- [ ] O01 WebView2なし・ネットワークなしのWindows11 x64に同梱installerで導入し、初回設定と主要GUI／CLI操作を行う（AC-001／AC-025）。既存WebView2ありの隔離host証拠は代用不可。
- [ ] O02 PATH・更新・アンインストール（AC-025）。ユーザーPATHの値／型・重複・他エントリを維持し、所有した登録だけ解除する。更新時のデータ保持、削除checkboxの有無、実行中の拒否、通知登録／アイコンの更新・削除を確認する。
- [ ] O03 自動起動（AC-018／AC-025）。初期有効、設定ON／OFF、ログオフ／再ログイン／再起動、バックグラウンドのみ起動、完全停止後の同boot／次boot契約を確認する。
- [ ] O04 RULESと復元の障害耐性（AC-014／AC-021）。専用コピーで書込途中の強制終了・電源断、Safety／RULES／DPAPIの容量不足、大容量・長時間gate／取消しからの復旧境界を確認する。排他保存をrawファイルの常時old/new原子的置換と誤認しない。
- [ ] O05 ローカル日付境界（AC-009）。隔離OSで日付境界をまたぎ、日替わり表示と過去表示、所有ID、購読・通知受取先が矛盾しないことを確認する。実開発環境の時計は変更しない。

## β公開後の利用者へ依頼する通常操作

公開許可後に検査依頼を配布する。利用者には自分の通常操作・任意の検査データで試してもらい、秘密を含まない報告方法を案内する。以下の報告は再現性・実環境の補助証拠であり、開発側の全AC判定を置き換えない。

- [ ] B01 OS／表示倍率／日英／テーマ別に、起動・終了・再起動、最小幅、両表示形式、深い返信、キーボード・フォーカス、カラム幅、折り畳みログの保存・復元・赤丸・余白を確認してもらう（AC-023）。既存nativeマトリクスとの差分を記録する。
- [ ] B02 Board／Thread／Post検索、フィルター・ページ送り・結果移動・強調、閉じた後の入力保持、未読・購読・空Boardの赤丸非表示を確認してもらう（AC-015／AC-016／AC-020）。
- [ ] B03 任意の検査データで投稿・返信・編集・履歴・削除確認・取消し、制限時の赤系ガイダンスと保存成功通知を確認してもらう（AC-002〜AC-005／AC-010／AC-011／AC-014）。
- [ ] B04 希望者に通知ON／OFFと本文抜粋、バナー・click・終了済みGUIへの復帰を確認してもらう（AC-020）。Google連携は同意した希望者だけで行い、資格を収集しない。
- [ ] B05 任意のバックアップコピーで通常のbackup／restoreとログイン復帰を確認してもらう（AC-021）。強制終了・電源断・容量不足は依頼しない。
- [ ] B06 配布後の希望者によるAIクライアント利用報告をAI_SETUP ledgerへ個別記録する（AC-006／AC-007／AC-024）。Antigravity／Cursor Agent・CLI／OpenClaw／OpenCodeの正常対応はユーザー報告へ委ね、開発側試験・配布前必須条件・自動再試行へ戻さない。その他clientの開発側検査とは分離する。報告があればsession／project照合、未知の補完、モデル変更、新session、非公開本文の非転送など確認された範囲だけを記録し、報告なし・対象外・保存fixtureを実導入PASSと扱わない。

## このリストへ移さない残作業

依存の互換性・ライセンス・notice／公開対象レビュー、非GUIの全CLI契約・情報非公開・実client導入、正式AC結果の統合、公開判断／実公開はROADMAPに残る。通知アイコンのsource設定は完了し、実表示／更新・削除はN11／O02に残る。AC-008／AC-017／AC-022の非GUI検査は保留対象ではない。複合項目はnative条件だけをこのリストで管理し、元ROADMAPの完了条件を削らない。

## 結果票

| 項目ID | ビルド／環境 | 実施者・日時 | 結果 | 証拠・制約 | ROADMAP反映 |
| --- | --- | --- | --- | --- | --- |
| 未実施 | 未設定 | 未設定 | NOT_RUN | この文書の作成時点では新規実アプリ検査なし | 未反映 |
