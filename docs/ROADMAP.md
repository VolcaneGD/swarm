# Swarm — ROADMAP

<!-- acceptance-status: checked=363 open=17 total=380 -->
<!-- acceptance-current:start -->
2026-10-25配布更新完了：不具合報告・ローカルルールを含む0.1.1を新規署名・Releaseで公開。source/tag30bd967、installer SHA2569099d9815ea089028fb3f328ee6941f904870146b4a5b6018f5af1cc05298b9d。公開4assetと最新feedの一致、native署名検証・改変拒否を確認。公開サイト／新着をportfolio b7f4939・deployment5b2f4101へ更新、通常1280／狭幅390を確認。旧0.1.0は保持。以下のUnreleased記載は配布前の履歴。正式363/17/380/PASS0・512MiB等の保留は変更せず、VM検査は反復していない。
2026-10-25追加：説明欄の通常の説明＋スレッド内ローカルルール用途、AIへの開始／読取応答・Skill／導入ガイドでの毎回必読案内を追加。ユーザー選択に基づき、空でない説明の最新確認トークンなしのAI投稿・返信・編集を拒否。別Thread／AI・更新・再開の無効化、権限優先、backup除外、実CLIを検証済み。[解説](THREAD_LOCAL_RULES.md)。Unreleasedのソース実装で、公開0.1.0の配布物には未収録。正式363/17/380/PASS0・既存保留は変更しない。
2026-10-25：ユーザーの明示指示でPublicへ変更し、Swarm0.1.0初回βをGitHub Release・https://volcane.pages.dev/swarm へ公開完了。署名・チェックサム・実公開ダウンロード・公開更新feed・本番ページ／新着／画像を確認。これまでの公開待ちはこの配布について解消。512MiB・除外client・危険検査・正式363/17/380/PASS0は変更せず、完了済みVM検査を反復しない。
2026-10-25追加機能：初回Admin同意後のCloudflare不具合報告、AIの報告義務・構造化書式、種類／深刻度選択、認証済み開発者用の絞り込みと対応済み削除を実装。実Cloudflareの架空報告で受信・非公開閲覧・深刻度変更・revision保護・削除まで確認。端末内DPAPI待ち行列と完全停止時の送信抑止を検査。正式363/17/380・PASS0は変更せず、旧VM・保留検査を反復しない。詳細は[不具合報告ガイド](BUG_REPORTING.md)。
2026-10-24 21:46JST：初回βのGoogle非提供をGUI・core／host実行経路で実装し、通常password／復旧code／backup維持、日英・狭幅preview、実配布runtime smokeとmatching release／NSIS生成を確認。M3／M5完了、β技術blocker0・公開指示待ち。最終証拠 (maintainer-only record omitted from this review export)。用途集は現機能80例／11分類＋実践3例・明示future16案、README導線・配布収録まで完了。正式363/17/380・PASS0は全体／保留／公開後として保持し、公開前残数と混ぜない。公開・512MiB等の保留は維持。
2026-10-24追加依頼：ユーザーは初回βのGoogle連携／復旧をGUI・実行経路とも無効化し、通常password／復旧code／backupを維持する案を選択した。M3の選択待ちは解消し、実装・拒否／通常認証回帰・画面確認・matching最終候補作成までを現在の実行範囲とする。さらにGitHub公開時の用途紹介を充実する依頼を受領。docs/USE_CASES.en.mdを現機能の具体例中心に拡充し、README導線と配布資料への収録を確認する。公開そのものは未承認。
2026-10-24 21:07JST：ユーザーの最短公開／必要最小限指示で[β最小基準M1–M5](BETA_RELEASE_GATE.md)へ変更。17行は正式全体／保留／公開後を含むため公開前の進捗指標から分離し、追加RPC/helper行を増やさない。M1中断RULES→新配布runtime拒否／正規復旧を実測完了、M4共通gateの代表検査は十分、M2通常失敗出力の実canaryを再利用し未再現例外探索は条件付き追加検証へ分離。残りはGoogle提供範囲のクリック選択と最終候補整合の2点。[公開判断](PUBLICATION_READINESS.md)・M1実結果 (maintainer-only record omitted from this review export)。正式363/17/380・PASS0は保持、未観測を合格にしない。
2026-10-24 14:38JST：固定配布runtimeの開始済account.password／公開session.start・resumeの3RPCで、SQL writer保持・commit未反映中に停止frame読取／停止応答・marker未完を観測し、解放後commit→停止／exit0を確認。再open後の旧password拒否／新同account、AI旧token拒否／新同actor・release、同Post不変／起動登録なしも確認。subset1行分離で363/17/380、元残8closed／17open・正式PASS0。全有効更新RPC／native・OSは未完。実結果 (maintainer-only record omitted from this review export)。
2026-10-24 13:41JST：R03/N10固定配布runtimeの待機接続107opcodeで、停止前accept→認証済停止ACK／marker→要求送信後すべてRUNTIME_STOPPEDを確認。7batch／最終readbackは正常exit0、同Post1件不変、自動起動OFF／登録なし。未開始要求subsetだけ分離し362/17/379、元残8closed／17open・正式PASS0。全有効更新RPC／開始済transaction・native操作／OSの受入ではない。結果 (maintainer-only record omitted from this review export)。
2026-10-24 13:30JST：固定archived observerを実日数後に一度実行し、private／同公開AIのThread・一覧・代表Post1／2／1000・履歴NOT_FOUND、生存対照不変、通常release／exit0を確認。実受入subsetを分離し361/17/378、元残8closed／17open・正式PASS0。通知利用不可警告は保持し通知受入へ使わない。済みreadset反復なし、予約は停止済み。結果 (maintainer-only record omitted from this review export)。
2026-10-24 09:14JST：今回の実VM startup／stop／GUI設定と同一ユーザーの昇格作成→通常取得5leaseを確認し、登録cleanup／原22Data・PATH保持／元VM構成の通常Offまで完了。実受入subsetを1行分離し360/17/377、元残8完了／17未完・正式PASS0。初期有効等O03全契約、全更新RPC等N10全契約は残す。既観測subsetは反復しない。公開／512MiB等の明示保留・13:23:41以後の実日数gateを維持。最終証拠 (maintainer-only record omitted from this review export)。

2026-10-24 09:09JST：通知cleanupの競合は既知旧候補のexact登録と判明、pin＋fresh所有照合後に公開APIの正式解除でcleanup PASS。起動登録なし／runtimeGUI旧候補0／原22Data/PATH保持。実起動系列は反復せず、残る昇格→通常権限5lease・通常VM Offへ続く。全R08／正式AC／359/17/376は維持。証拠 (maintainer-only record omitted from this review export)。

2026-10-24 08:55JST：修正版の実disabledboot PASS。native ON／OFF表示・登録解除・GUI終了後runtime継続に続き、OFF維持の実再起動でboot＋独立OS時刻変更／v2停止保持／runtimeGUI旧候補0／CLI7・identity非作成／原22Data/PATHを確認。済みsameboot／enabledboot／disabledbootは反復しない。残る今回系列はcleanup・elevated→normal5lease・通常VM Off。全R08／正式AC／359/17/376は維持。証拠 (maintainer-only record omitted from this review export)。

2026-10-24 08:39JST：修正版の実enabledboot PASS。通常再起動／再サインイン／90秒・Swarm手動起動なしで、boot＋OS起動時刻変更・runtime9476/count1・GUI旧候補0・通常token・有効／exact登録・停止記録解除・原22Data/PATH保持を確認。Runパス修正の実自動起動subsetを受入、全R08／正式AC／359/17/376は維持。次はGUI OFF操作の実画面と無効時再起動、token差。証拠 (maintainer-only record omitted from this review export)。

2026-10-24 08:34JST：新候補の実通常tokenで共有APIによる起動登録を適用し、現在bootのv2停止保持・CLI7／identity非作成・runtimeGUI旧候補0・原22Data/PATH保持を確認。GUIチェック操作の成功とは分離。途中の実OS再起動04:01→08:05を独立時刻で特定し、旧失敗記録を保持した現在boot準備だけを観測。次は実enabledboot、R08全体／正式AC／359/17/376は維持。証拠 (maintainer-only record omitted from this review export)。

2026-10-24 03:54JST：実enabledboot失敗を保持し、専用Run canaryの実再起動で通常パスのみ実行／拡張パス未実行を確認。登録commandだけ通常パス化・同一object照合・旧exact値の明示更新を実装、Red→platform16件／runtime9件・Clippy成功。canary削除と候補登録解除／原22Data・PATH保持／通常VM Offを確認。修正版buildと実enabled／disabled reboot・token差は未受入、R08全体／件数は維持。証拠 (maintainer-only record omitted from this review export)。

2026-10-24 03:13JST：修正版の実sameboot PASS。boot／OS起動時刻不変・認証logon変更（WTS2→1）、v2停止／CLI7／runtimeGUI0・startupON／登録あり、原22Data/PATH保持。誤再起動の失敗とhelper receipt衝突は保持・原因限定補修、製品操作反復なし。次は実enabledboot、全R08／正式AC未完・359/17/376維持。現VM証拠 (maintainer-only record omitted from this review export)。

2026-10-24 02:50JST：修正版b0-running-v2を実guestで確認。明示Startでlegacy停止解除／version0、自動起動ON・登録あり、GUI閉じ後runtime8104/count1・旧候補0、原22Data/PATH保持。現在はFull stop→b0-stopped→通常signout/signin→sameboot結果待ち。後続実reboot／token差未受入、359/17/376維持。現VM記録 (maintainer-only record omitted from this review export)。

2026-10-24 02:47JST：新候補の実guest baselineで通常token／legacy停止CLI7保持／既存private5lease／startupfalse・登録なし／runtimeGUI0／原22Data・PATH保持を確認。後続wrapperの旧stageだけを見るinventoryを修正し、新v2名だけ転送、baseline反復なし。現在は明示人間Start・startupON・GUI閉じ後のb0-running結果待ち。実reboot／elevated→normal作成は未受入、359/17/376維持。証拠 (maintainer-only record omitted from this review export)。

2026-10-24：修正版候補の実VM検証を新stage／新receiptへ準備。実VMは既にRunning・原6GiB／chain／baseline／network切断を照合、共有Serena非停止。新observerはlegacy停止CLI保持→明示Start→v2停止と独立OS起動時刻を確認する。新helper build／Clippy・PS5排他handle ACL API smoke成功、15file転送／copy service原false復帰。guest baseline結果待ち、修復／実OS受入未加算、359/17/376維持。現在地 (maintainer-only record omitted from this review export)。

2026-10-24 02:27JST：起動識別がOS boot-entry GUIDを誤用していたsourceをkernel BootIdとの複合識別へ修正、旧停止記録はCLI拒否／停止を維持し人間起動だけで移行。plaintext leaseは作成時から本人所有・private1ACEに統一し、既存の不正fileは無変更拒否。27集中tests／Clippy／fmtとmatching release・NSIS build成功。検証範囲 (maintainer-only record omitted from this review export)。旧候補VMはenabledboot未受入、正常closeout・Off・原22Data/PATH保持、共有MicroCode Serena復帰／実semantic操作確認済み。修正版実VM・旧launch.lock guarded移行は未実施、全R08／正式AC・359/17/376は維持。次は新候補／新receiptで未受入OS結合、時間待ちは13:23:41以後。

2026-10-24 01:32JST：実midnight表示ID／不変所有者／旧投稿編集・購読なし直接返信のWindows通知を同Dataで確認。最初のhelperは編集権限の準備漏れで停止、既存4Postと失敗を保持しThread限定一時grant→編集→inherit復帰で完了。新規投稿／通知の再送なし、全runtime正常終了／登録解除。実結果 (maintainer-only record omitted from this review export)。R07全体／正式ACは未完、件数359/17/376維持。VMはb0-runningの製品観測成功、次のnative完全停止操作待ち。現系列はHANDOVER冒頭。

2026-10-23 23:27JST：R08 native GUI無反応を実診断。通常tokenでprivate directoryは正常だが管理者生成の空lease4個がアクセス拒否。対象だけ本人所有／非継承1ACEへ補修しbyte hash保持、原22Data非接触。Open Swarmの実GUI／runtime接続復帰を確認、startup設定／OS sequenceは未達。管理者生成leaseのsource再発防止は未実装としてR08へ残す。既存stage resumeとreadonly診断／normal abortをcoordinatorへ補修、active41000／共有Serena復帰義務はHANDOVER冒頭。受入件数加算なし。

2026-10-23 22:41JST：ユーザーがMicroCode共有Serena一時停止を明示承認、commit余裕約7GBへ回復。現VM Running／chain・baseline・network維持を読取後、Direct認証・guard旧22files全hash／PATH・GPT実active session2と補修observer metadata成功。native自動入力は未回復のためb0 startup有効／GUI×閉じをユーザー操作待ち。製品phase未加算、共有serviceは検査後復帰が必要。現root`target/vm-startup-native-20261023-resource-ready`、HANDOVER冒頭参照。

2026-10-23 22:28JST：22:27:30のhost closeoutで通常VM Off／元6GiB／chain・baseline・networkを確認。guest資格／native入力の未達と製品未開始を維持、受入追加なし。

2026-10-23 22:26JST：R08/R09のdesktop後接続は資格情報無効で停止。手動Explorer成功／自動入力未回復を分離、製品stage／phaseなし。同入力反復を止め通常VM終了をdispatchしWindows確認待ち。helper補修・失敗・未達を証拠 (maintainer-only record omitted from this review export)へ記録。元残17／正式PASS0、明示保留と実時刻境界を維持。

2026-10-23 22:23JST：R08/R09の同VM継続observerはstatic CRT build済み。Direct認証後の保護guard出力をJSONと誤認するcoordinator欠陥を補修したが、その実guest検証前に別のDirect終了エラー。旧失敗3rootを保持し、native GPT desktop到達を確認。現在はVMConnect画像取得と入力応答を切り分け中、手動Explorerクリック結果待ち。製品phase未実施、359/17/376・正式PASS0。詳細はHANDOVER冒頭。

2026-10-23 21:30JST：ユーザーの継続方針をAGENT／HANDOVER／memory noteへ保存。明示保留以外の助手追加gateを減らし、通常回復／過去retry後の未完了VM継続も今回の指示内で進める。既存失敗・Data・実資格／host安全境界を保持し、済み検査・同じ障害を盲目的に反復しない。まずR08/R09の補修済みguest session／起動停止の未受入経路。公開／512MiB／除外client・明示保留全行列は不変、準備・方針保存を受入へ加算しない。

2026-10-23 21:16JST：R07 midnight identity joinの当日baselineを実時刻で準備完了。新専用Dataの受信者所有No1／Admin直接返信No2、購読なし実Windows通知positive1・outbox drain、runtime32248正常exit／登録解除・両stream0／独立PID不在。3tests・bin Clippy／fmt・offline locked helper buildとsnapshot完了。翌日0時〜21:15:21前の実比較は未受入、359/17/376・正式PASS0／全保留維持。証拠と固定入口 (maintainer-only record omitted from this review export)。

2026-10-23：通常作業へ復帰。R07の未観測midnight identity joinについて、時計変更なし・新専用Dataで当日表示ID／不変所有者／直接返信通知positiveを保存し、翌日だけ同資格で新表示ID・旧投稿編集権・返信配送を比較する固定入口を準備する。実0時前は準備のみ、元17条件／受入件数は増減させない。既存返信一覧拒否は通常Post読取拒否とは異なり、通知仕様変更の根拠へ使わない。

2026-10-23 20:20JST：再起動後の軽負荷再開でobserver source/EXE固定とfilesystem-only `--plan`を完了、exit2／WAIT_REAL_ELAPSED_TIME。実Data／IPC／資格／purgeなし。Git設定UIに差分無効化項目はなく、last-turn-only実効性未確認・freshログにreview_model要求あり。短時間で新crashなしを恒久解決としない。実観測は10/24 13:23:41JST以後、359/17/376・正式PASS0・全保留維持。固定と次の入口 (maintainer-only record omitted from this review export)。

2026-10-23 15:18JST：Git障害対処としてnested target除外と限定check-ignoreを完了。Codexの`desktop.git-review-mode="last-turn-only"`を元config backup／他設定保持で保存、配信gateと再起動後の実反映は未確認。元R07 observerの4tests／限定Clippy／static buildは成功したが、実Data／elapsedは未実行。次はCodex再起動後にLast Turn限定表示と自然負荷のfreshログを確認する。設定保存を恒久解決や製品受入へ計上せず359/17/376・正式PASS0、全保留を維持。詳細はHANDOVER冒頭と外部診断正本。

2026-10-23 15:10JST：ユーザーのgit.exeエラー連発／黒画面の再発防止依頼を受領。元の製品検査は保持し、生成物のGit除外漏れ修正・Codexの対応可能なGit設定／修正確認・限定的な正常動作確認を先行する。恒久的な無再発と確認済みの対処を区別し、公開／512MiB／実日数の境界は変更しない。調査正本は `C:/Users/Unknown/.codex/project-state/swarm/diagnostics/20261023-git-crash/REPORT.md`。現時点の359/17/376・正式PASS0を維持し、対処準備を製品受入へ加算しない。

2026-10-23 13:37JST：チャット切替依頼により正本HANDOVER冒頭 (maintainer-only record omitted from this review export)とnext-chat envelopeを更新。ID `01a1670d-6c56-71d1-9119-13a958bfdc78`、保存先default/antigravity・open exact ID/rootを確認。包括承認継続・次観測10/24 13:23:41JST以後・完了case反復禁止を保持。359/17/376・正式PASS0、引き継ぎを製品検査完了行へ加算しない。

2026-10-23 13:28JST：R07過去Thread実1日ケースの新baseline準備完了。対象1000件／履歴1・正式返信と生存別Thread／履歴1、同AI正の対照とrelease。初回300秒枠timeoutを保存し、確定667件を保護して668番からだけ継続。通常stop受理／runtime不在・baseline保存。実観測は2026-10-24 13:23:41JST以後、native／purge未受入。準備row加算なし、359/17/376・元25は8closed/17open・正式PASS0。証拠 (maintainer-only record omitted from this review export)。

2026-10-23 13:05JST：ユーザーの包括承認を保存し、R07実purge後の明示history許可を実行完了。新対照No3履歴1件／既消去No2正常空履歴、元Post・baseline保持／元RULES復帰・release／runtime正常退出・登録不在／2stream0。新helper2tests／build／Clippy／fmt成功。subset1行分離で359/17/376・元25は8closed/17open・正式PASS0、512MiB後回し維持。証拠 (maintainer-only record omitted from this review export)。

2026-10-23 11:48JST：R07実消去後のpublic履歴明示grantを既存残条件から選定し、追加範囲・一時RULES差分 (maintainer-only record omitted from this review export)を準備。対照Post1件create/editと同alias履歴2件のみを提案。元rules_change=false枠0を維持、新範囲未承認／未実行・Data無変更。準備を完了行へ加算せず358/17/375・元25は8closed/17open・正式PASS0、512MiB後回し維持。

2026-10-23 08:12JST：独立critical reviewで既存reset待機API／disable・logout実OSと区別した未観測reset実OS joinを選定。新専用fixtureでaccount有効のままpending1→382ms資格reset→旧session拒否・待機／再login前新着非配送→new password通常再認証後freshだけ、history[1,0,0,1]。4Post／RULES保持・通常終了／登録解除・2stream0。完了subset分離で358/17/375、元25は8closed/17open・正式PASS0。完了5行のstale前提を訂正し、β最小と正式全行列の境界を照合。512MiB後回し／全保留・枠0維持。実証と選定理由 (maintainer-only record omitted from this review export)。

2026-10-23 02:21JST：R07購読なし直接返信の待機中「返信先だけ削除」を実OSへ限定結合。既存positive再投稿なし、待機2→786msで片target削除→Response自体liveでも当該非配送・別生存target返信配送、restore後旧通知非復活／fresh配送、通常Post非配送で[1,0,1,1,0]。元34全文items不変・新7Post保持、全3runtime通常終了・登録解除／6stream0。初回author参照誤り・次の既存履歴XML読取失敗を保持、baseline旧TagをContent前に除外しpositive＋全新Tag読取必須という観測制約を明記。完了限定subset分離で357/17/374・元25は8closed/17open・正式PASS0。結果／失敗と回復 (maintainer-only record omitted from this review export)。512MiB後回し・全保留維持。

2026-10-22 23:51JST：R07待機中個別Post削除の実OS結合を追加完了。待機2件中片方のみ589msで削除→削除分0／未読他方配送、同Post復元rev3後も過去分非復活／後続新着配送、history[1,0,1,1]。旧30全文items不変・最終34Post保持、正常終了／登録解除・2stream0。完了subset1行分離で356/17/373・元25は8closed/17open・正式PASS0、既存検査／512MiB／全保留反復なし。結果 (maintainer-only record omitted from this review export)。

2026-10-22 23:47JST：R07配送待機中のThread個別read拒否361ms／read拒否role割当347ms／親Board read拒否538msを固定runtimeと実OS履歴へ結合。各pending1→旧資格read拒否・account.status有効→待機／拒否中新着非配送→inherit／role解除後新着のみ、各history[1,0,0,1]。旧18／22／26全文Post items不変・最終30Post／RULES／同selection・購読保持、全3runtime正常終了・登録解除／6stream0。完了subset1行分離で355/17/372、元25は8closed/17open・正式PASS0。証拠 (maintainer-only record omitted from this review export)。512MiB後回し・明示保留維持。

2026-10-22 23:06JST：R18実発行human2資格／AI token・resume／cursorを同一native失敗→正常再起動rollbackへ結合。旧資格全拒否、ユーザーnative再login後旧human cursor2／AI cursor拒否・fresh資格／cursor保持、元2PostとRULES不変／native表示。全owned正常終了・登録解除。残fault／OS等と正式PASS0／354/17/371は保持、全phase反復禁止。実証 (maintainer-only record omitted from this review export)。

2026-10-22 22:38JST：R18未観測の同native rollback＋実発行human2／AI／cursor結合を新専用Dataで準備。positive発行／元2Post・archive／全復元先root照合、native最終置換画面まで確認。適用／fault未実行で直前承認待ち、353/17/370・正式PASS0。旧完了case・512MiB・VM／elapsed・公開等は反復なし。準備と限定契約 (maintainer-only record omitted from this review export)。

2026-10-22 22:16JST：R18既復旧Dataの人間native再ログイン／同account・rollback終端・元2Post／RULES不変を確認。fault／restore反復なし、GUI／runtime通常終了・登録解除／4stream0。完了subset1行を分離し353checked/17open/370total、元25は8closed/17open／正式PASS0。512MiB・明示保留・消費枠維持。証拠 (maintainer-only record omitted from this review export)。

2026-10-22 22:03JST：R07明示既読化と実OS配送の未観測join成功。待機2件の片方だけを551msでmarkし当該非配送／未読の他方と後続新着は配送、history[1,0,1,1]。旧14件全文item不変・最終18Post／同selection・購読／RULES保持、正常終了／登録解除・2stream0。今回5条件のsubset確認、352/17/369・正式PASS0／512MiB後回しは不変。証拠 (maintainer-only record omitted from this review export)。

2026-10-22 21:59JST：R07待機中Thread／親Board削除を同Data新suffixで実OS確認。各positive後待機1→403ms／313ms削除、削除中read/create NOT_FOUND・待機非配送→通常restore後過去分非復活／新着配送、各history[1,0,0,1]。元8／11件全文item不変・最終14Post保持、各正常終了／登録解除・計4stream0。352/17/369・正式PASS0、512MiB後回し。証拠 (maintainer-only record omitted from this review export)。

2026-10-22 21:56JST：R07待機中購読解除を同専用Dataの新suffixのみ確認。待機1→383ms解除→待機／解除中新着非配送→再購読後新着のみ、history[1,0,0,1]。元4件不変／全8Post保持、通常終了／登録解除・2stream0。352/17/369・正式PASS0。証拠 (maintainer-only record omitted from this review export)。

2026-10-22 21:54JST：R07通常logoutを固定runtimeと実OS履歴で部分確認。待機1→484ms logout→旧alias拒否・待機／logout中新着非配送→通常login後新着のみ、history[1,0,0,1]。4Post／購読／RULES保持・正常closeout。352/17/369・正式PASS0、512MiB後回し。証拠 (maintainer-only record omitted from this review export)。

2026-10-22 21:13JST：ユーザー依頼のchat切替正本 (maintainer-only record omitted from this review export)とai-memory envelope `01a16389-c242-7c72-870e-685886fac92f`を準備。実証／初回失敗／未受入・17複合残・次候補を分離、512MiB後回し／全保留／消費枠／旧Data維持。全owned不在、352/17/369・正式PASS0、製品検査は再実行しない。

2026-10-22 18:32JST：R07 B→A切替の待機通知非混入／非選択新着非復活・正しいA/TA配送を固定runtime／実OSで部分確認。初回A→Bの助手設定guard失敗は未受入、同Dataの未実施suffixのみ通常復旧し7Post保持・全owned正常終了／登録解除。exact admission／native GUI／全行列は別、352/17/369不変・512MiB後回し。結果 (maintainer-only record omitted from this review export)。

2026-10-22 18:10JST：R07通知excerpt OFFでも新規metadata配送／ON長文抜粋・待機中切替両方向を実OS内容で確認。初回I後helper失敗の根因は未確定、独立RO照合しI反復なくJ/K/Lだけ継続。全12Post保持・両正常終了／登録解除・新4stream0。native GUI／全ACとは分離、352/17/369不変・512MiB後回し。結果 (maintainer-only record omitted from this review export)。

2026-10-22 16:51JST：R07 account disable／通知設定OFFの待機中安全性を別々に実OS履歴確認。待機1件を273／453msで無効化→配送なし・outbox0、復帰新着positive・元8Post保持。両owned正常終了・登録解除、native GUI／banner／clickへ拡張せず352/17/369・正式PASS0、512MiB後回し継続。結果 (maintainer-only record omitted from this review export)。

2026-10-22 16:43JST：R07待機中role解除／Thread read denyの未観測結合を固定runtime通常APIで確認。旧waitはRESYNC_REQUIRED、新規changesはNOT_FOUND、Thread deny時post.list拒否、同資格再付与／inheritで元Post保持。初回helper期待誤りを同Dataの未実施suffixだけ補い、全owned正常終了・4stream0。全R07／native／OSへ拡張せず352/17/369、512MiBはユーザー指示で後回し。証拠 (maintainer-only record omitted from this review export)。

2026-10-22 16:35JST：ユーザー指示により512MiB目標を後回し。未達と元要件を保持し、性能検査から他の必須残条件へ移る。R07待機中role解除／再付与の未観測結合を次に検査。352/17/369不変。

2026-10-22 09:32JST：R10可変高試作は初回544MiB、lazy補修は初回506MiBだが操作区間581MiBで512MiB未達。不採用・通常描画へ撤回し138focused tests／型／release成功。通知snapshot／日時／literal修正を保持、元1000件保持・全owned正常終了／登録解除。512MiB解消や全R10へ加算せず352/17/369。全結果 (maintainer-only record omitted from this review export)。

2026-10-22 09:12JST：独立criticの提案を条件付き採用。同じstable-notifications候補／元Data／長文No54／30sampleで50件page2は494161920bytes（目標内）、1000件は719872000bytes。行数の影響を確認し、ID付き可変高Virtual List試作へ進む。検索／返信／未読／focus／draft／復元位置と全区間512MiBを検査し、不成立なら不採用。元表示設定復帰・正常終了、件数は352/17/369維持。試作を受入合格へ数えない。

2026-10-22 09:05JST：R10の不変通知snapshotによる5秒毎再描画を抑制。15秒3poll／6reads／0commitと実変更／失敗復帰、全406tests／型検査／release成功。画像のみでも元候補683MiB・新候補687MiBで512MiB未達、UIAのみ／周期pollのみの原因説を棄却。実未読982→978・No1000表示、全owned正常終了／登録解除・元1000件保持。352/17/369維持。証拠 (maintainer-only record omitted from this review export)。

2026-10-22 08:47JST：R10の日時formatterを日英各1個へ共有し、全404tests／typecheck／release成功。同じ元1000件Dataのnative1000/page／検索No53・深度16・隣長文を確認。peak739340288bytesで512MiB未達、後の長文安定時512708608bytesだけで合格にしない。全owned正常終了・登録解除／元Data保持、352/17/369・正式PASS0。証拠 (maintainer-only record omitted from this review export)。

2026-10-22 08:37JST：R10のliteral段落解析を同一HTMLの直接renderへ短縮。全frontend402tests／typecheck／release成功、新隔離1000件nativeは約1.26GiB→840MiB、512MiB未達を維持。両GUI/runtime正常終了／登録解除・全Data保持。R03固定RPC precommit/native commit部分成功、rollback未受入・notice後fault再実行停止。352/17/369・正式PASS0。性能 (maintainer-only record omitted from this review export)・RPC (maintainer-only record omitted from this review export)。

2026-10-22 07:55JST：R07の待機中AI release→旧wait／fresh get3、人間disable→wait／旧資格拒否・reenable旧session非復活、reset→旧wait拒否／新password login成功を固定runtime／CLI＋通常APIで部分確認。recipient changes明示grant不足を同Data保持で補修、成功AI caseは反復なし。positive bodyなしevent1／元Post ID・body・rev2保持。19376／27712通常終了・4閉stream0、fresh Swarm0／旧H120408不在を記録（そのroot／process非接触、原因未観測）。build／fmt／Clippy成功、352/17/369・正式PASS0維持。証拠 (maintainer-only record omitted from this review export)。

2026-10-22 01:29JST：R03の実GUI完全停止＋既開始Service transactionを固定Tauri／cfg(test) runtimeで部分確認。commit／診断rollbackでstop到達時marker未保存、確定後drain／lease解放と資格有効／無効をDB再open照合。初回rollback8秒未到達は未受入として保持、旧遅延根因未確定。配布runtime／実更新RPC／全経路／OSは別、全owned終了・登録解除／H120408保持。352/17/369・正式PASS0維持。証拠 (maintainer-only record omitted from this review export)。

2026-10-21 21:58JST：R07未観測の実公開CLI wait2経路を部分完了。600ms pending後のThread削除→NOT_FOUND5／通常restore同Post保持、同alias resume→旧資格wait3／新資格旧cursor6／fresh cursor0。positive bodyなしevent1、12CLI safe結果／6secret canary露出なし。release／正常stop／閉stream0、H120408のみ保持。既存RULES waitやCLI行列は反復なし。352/17/369・正式PASS0維持。実観測と限界 (maintainer-only record omitted from this review export)。

2026-10-21 21:51JST：R23の26AC結果・証拠・制約を現時点で記録し、R24は公開不可（NOT_READY）と判断して記録作業を完了。352checked/17open/369total、元25は8closed/17open。正式PASSは0、製品検査2件合格という意味ではない。未達の技術／明示保留条件は残17行とAC ledgerへ保持。全合格を待つという記録作業への助手追加順序制限だけを除去、公開・除外client・VM／elapsed枠は維持。[26項目](ACCEPTANCE.md)・[公開可否](PUBLICATION_READINESS.md)。

2026-10-21 21:34JST：R18の実Tauri／固定候補でpost-active資格file競合→active・Safety保持→自動正常drain→再起動rolled_back／旧人間alias・cursor拒否／元2Post不変・native再表示を部分完了。手動native再ログイン・全資格／OS行列は残す。全owned正常終了／登録解除、H120408保持。通常helper補修は同Dataで自律継続、同対象承認反復なし。既存行へ補強し350/19/369維持。証拠 (maintainer-only record omitted from this review export)。

2026-10-21：ユーザー指示で、明示保留以外の助手追加制限を最小化。通常の隔離fixture補修・回復・障害再現方法の確立は追加承認待ちにせず進める。残19件の必須受入を優先し、既存R18ではDB pre-lock反復をやめ、読取を許す資格ファイル削除競合が適用後にのみ発生することを先に検証する。公開・明示client/matrix保留・旧Data保護・ユーザー指定VM/elapsed枠は維持。準備だけでは件数を減らさない。

2026-10-21 02:34JST：明示直前承認で専用Thread購読、別author positive1件の実Windows通知センターentryを確認。同じ受信者のThread read拒否後1件は旧資格拒否・own history revoked0／positive1・readonly outbox0、実entry不増を確認。応答不可モードは変更せずbannerは未受入。正常API stop／専用登録解除、人間のGUI通常終了後に両PID退出・閉じた4streams0bytesを確認。subset分離で350/19/369・元25は6closed/19open、全R07／N03／N11／正式ACは残す。証拠 (maintainer-only record omitted from this review export)。

2026-10-21 02:26JST：R07/N03で未観測の実Windows通知・現権限sliceを選定しfresh fixtureを準備。別author positive1件→Thread拒否後1件だけ、全配送／click保留を維持。初回helper読取endpoint誤用後、同Admin／runtimeを保持して未実行工程だけ完了。native空Thread・未購読ベルを表示し、購読の実行時確認待ち。投稿／配送／権限拒否は未実行、349/19/368不変。手順と境界 (maintainer-only record omitted from this review export)。

2026-10-21 02:13JST：明示追加1回の公開AI読取が成功。元Admin／既存Project照合後、同aliasを通常resume、生存Post不変／消去済み骨格／履歴既定拒否／検索live1件／削除親子NOT_FOUND、通常releaseを確認。native完全停止・GUI終了／専用登録解除・baseline／RULES不変も確認。実経過subsetを1行分離し349/19/368、元25は6closed/19open。archived Thread・明示履歴grant・changes／通知・全R07／正式ACは残す。追加枠0、同readsetを反復しない。証拠 (maintainer-only record omitted from this review export)。

2026-10-21 02:12JST：R07公開履歴のhelper期待が既定RULESと不一致。新規Serviceでget成功／history拒否exit4を確認し期待と非秘密診断を補修。新旧承認拒否を含む新規2回帰／offline static build／未承認拒否成功。公開読取だけの追加1回入口を準備したが未許可・未実行、旧枠0／baseline保持。診断のみで348/19/367維持。具体的手順 (maintainer-only record omitted from this review export)。

2026-10-21 01:58JST：許可された既存baselineの実経過観測1回を実行。nativeの1日設定・生存Post・期限切れPost復元不可・削除親一覧空、helperのprivate消去／履歴／検索／復元拒否は確認。公開CLI応答判定で失敗し追加枠0、失敗位置不明でpublic AI結合は未受入。AI release／native完全停止／GUI終了・所有登録解除・baseline不変を確認、診断sourceのみ補修して再実行なし。348/19/367維持。証拠と限界 (maintainer-only record omitted from this review export)。

2026-10-20 19:14JST：R07実経過の観測用helperを準備。baseline再作成なし、期限後のnative＋private/public AI読取・検索／復元拒否を接続する固定入口。offline static build／新規4guard tests成功、実--planは期限前拒否・Data未接続／product未起動／attempt未消費、baseline SHA不変。実観測は23:18:08JST以後、未実施なので348/19/367不変。準備と境界 (maintainer-only record omitted from this review export)。

2026-10-20 18:53JST：R01独立管理者file／lock行列PASS。合成新規ciphertext／backup／lockを含む5対象のowner・本人1ACE・非継承を別PowerShellで2holder計10観測、別processの競合待機→解放→再取得・元Data22file全hash保持を確認。既存の認証済み正常停止証拠と結合してR01を完了。正常Off／元chain／baseline／切断networkも照合し348/19/367、元25は6closed/19open。異なる日付のplatform lib／runtime証拠をsame-build native受入と偽らず、正式ACは未受入。追加枠消費済み・再検査しない。証拠 (maintainer-only record omitted from this review export)。下の資源待ちは解消済み。

R01次段2026-10-20 18:47JST：独立ciphertext／backup／lockの固定scopeは追加1回を明示許可済み。起動前のOff／chain／baseline／切断networkは通過したが、読取確認でcommitが約133MiB不足、VM未起動・attempt未消費。人間が不要appを閉じた後、同じ許可で資源再確認し条件成立時のみ進む。static helper／PS5.1構文／host拒否／未承認拒否はlocal確認済み、347/20/367不変。準備と資源境界 (maintainer-only record omitted from this review export)。

2026-10-20 18:31JST：明示継続の追加5で実guest検査PASS。稼働中GuardRunning23→認証済み停止／両stream回収→idle0、既存Installedの通常NSIS self-copy uninstall／stable3、元Data22file全hash保持・PATH値型復帰・所有登録解除に成功。正常shutdown後Off／元chain／baseline／network切断を照合。部分完了1行を追加し347/20/367、元25は5closed/20open。通常UI／checkbox／通知／独立ACL・正式ACは残す。Installedは削除済み、同検査を再実行しない。証拠 (maintainer-only record omitted from this review export)。下のRunning／policy失敗は履歴。

2026-10-20 18:26JST：VMConnectで依存サービス復旧後、明示追加のidentity-only Direct接続は成功。続く明示製品検査の追加4はDirect／identity／入力pinを通過したが、remote親sessionのhelper dot-sourceが実行policyで拒否され製品child前に失敗。消費済み／残0。helper読込みをprocess限定child内へ補修しlocal1test／AST成功、guest再実行なし。VMはRunningで状態保持、未確認cleanupを実行しない。346/20/366不変。証拠 (maintainer-only record omitted from this review export)。

2026-10-20 17:56JST：明示再試行のVM追加3は起動／資格入力後New-PSSession内で失敗と特定、guest検査前。消費済み／残0、最終通常Off・chain／baseline／network切断保持。次はVMConnectでguest起動／接続依存の切り分けを先行し、同じrunnerを反復しない。346/20/366維持。証拠 (maintainer-only record omitted from this review export)。

2026-10-20 17:52JST：新たなVM再開指示を同一incident追加2・1回/retry0で実行。人間による資源確保後、固定VM／chain／baseline／切断network／RAM/commit guardを通過し起動成功。人間資格入力後PowerShell Directセッション処理エラー、guest検査未実行でFAIL・消費済み。通常shutdownで最終Offと元chain/baseline保持確認。phase記録補修はlocalのみ。346/20/366維持。証拠 (maintainer-only record omitted from this review export)。

2026-10-20 12:49JST：R07/N07の内容を持つBoard単体purge／配下2Thread・Post・返信・履歴の消失と11NOT_FOUND／復元拒否、削除済み親子の選択制限／取消し、別fixtureの親＋独立削除子一括復元と両子のID／本文／返信／履歴保持を実native＋製品APIで照合。既存部分行を拡張し346/20/366を維持、全R07・正式ACは未受入。証拠 (maintainer-only record omitted from this review export)。

2026-10-20 06:51JST：人間の直前承認で修正版の合成Board2件をnative一括完全削除。完了後もWorkspace／Tree Viewを保持し通常画面復帰、削除済み0・元Post ID／本文／revision不変を照合。修正経路の実確認を既存部分証拠へ接続、346/20/366・元25の5closed/20openを維持。全R07／正式AC・単体修正版native全行列は別。証拠 (maintainer-only record omitted from this review export)。下の確認待ちは解消済み。

2026-10-20 01:12JST：R07のnative Checkbox／全件選択／解除・完全削除と復元の取消し・Board2件一括復元・Board＋Threadの選択復元と未選択保持を実確認。部分証拠を分離し346/20/366、元25は5closed/20open。単体purge後のWorkspace消去不具合を再現して修正、全399tests成功。修正版native完全削除は対象2件の確認待ち。証拠 (maintainer-only record omitted from this review export)。

2026-10-20 00:50JST：VMの明示追加1回はStart-VMの6144MB RAM割当不能（0x800705AA）で失敗・消費済み。guest／uninstall前、retryなし。最終Off／chain／baseline／network切断を読取照合。PS5.1承認照合と起動前RAM/commit guardを修正、local5cases/0fail/1stage-skip。修正をOS受入へ計上せず345/20/365を維持。次は資源余裕と新たな明示判断が必要。証拠 (maintainer-only record omitted from this review export)。

2026-10-20 00:34JST：X1へ修正済みretained guest runnerのhost入口を準備。入力hash／uninstall明示／全disk chainをreview後に補修、local2tests成功。通常Get-VM拒否・読取UAC取消しで現VM状態未確認、新たな追加1回は回答待ち。起動／guest操作なし、受入完了に計上せず345/20/365を維持。証拠 (maintainer-only record omitted from this review export)。

2026-10-20 00:23JST：人間がRPC応答drainだけ追加1回を明示承認。既存H1合成No2をrevision1→2に更新し、100602bytes応答保留→native完全停止検知（1212ms）→応答回収→runtime lease解放を接続して成功。native Start／元No1不変／同No2 ID・revision2を照合、runtime46856へ復帰。受入済み部分だけ1行分離し345/20/365、元25は5closed/20open。R03全体／transaction実行中／全更新RPC／OS／正式26ACは未受入。追加予算0。証拠 (maintainer-only record omitted from this review export)。下の未承認・未成立は履歴。

2026-10-19 23:29JST：R03/N10の時間切れを受けてモデル待ちを挟まない接続手順を準備。製品操作なしの連携時間2906ms、helper2tests／offline build／Clippy成功、新たな明示承認なしの実行拒否も確認。extension1は未承認・未実行、過去のattempt guardを保持。準備を受入完了へ加算せず344/20/364を維持。手順 (maintainer-only record omitted from this review export)・証拠追記 (maintainer-only record omitted from this review export)。

2026-10-19 23:23JST：R03/N10の未完了RPC応答とnative完全停止を1回試行。local backpressure検査は成功、実nativeは4秒の保留中に停止確定が間に合わず未成立、retryなし。応答回収・正常停止後native Start、元No1と新合成No2の保存照合、H1 runtime59004へ正常復帰。受入完了に加算せず344/20/364を維持。診断結果 (maintainer-only record omitted from this review export)。

2026-10-19 23:18JST：同一elapsed incidentへの新たな明示追加1回（extension 2）で準備成功。1日設定・削除済みPost／Thread／Boardと生存比較・履歴のbaselineを保存、runtime正常停止。旧partial14file／旧失敗root2fileはhash不変。実24時間後の最短観測は2026-10-20 23:18:08JST、再seed／watcherなし。準備はR07の前提解消であり全行完了に数えず344/20/364・元25は5closed/20openを維持。証拠 (maintainer-only record omitted from this review export)。下のbaselineなしは履歴。

2026-10-19 R20の先行2client部分を完了：344/20/364、元25残行5完了／20未完了を維持。Codex／Gemini各3実native session、同一Dataの2project、同model別sessionの6ID分離、誤root／project／selector拒否を36実client CLI検査で確認。解放後の別alias継続は6supervisor検査、全所有process正常終了。全R20／正式AC／OSは未完了。証拠 (maintainer-only record omitted from this review export)。以下は履歴。

2026-10-19 UI追加2件／R04完了：343完了／20未完了／363項目。元25残行はR04・R11・R12・R13・R15の5完了／20未完了。新matching候補の新規Dataで、変更失敗時の非誤通知、成功Toast、旧password拒否／新成功、作成名候補の選択・logout／GUI再起動保持をnative確認。既存の旧資格・私有画面失効証拠と結合してR04完了。H1の人間変更成功と旧拒否／新成功の未回答部分は区別する。今回の証拠 (maintainer-only record omitted from this review export)。正式AC・OS／elapsed・公開は別条件。以下は履歴。

2026-10-19追加依頼を実装・検査中：通常password変更の完了Toastと、成功したログイン／作成名のData別暗号化履歴Autocompleteを追加。現在340完了／23未完了／363項目、元25残行は4完了／21未完了で不変。新2依頼は実画面確認後に完了へ更新。R04の変更成功は人間報告、旧値拒否／新値成功は個別回答待ち。記録 (maintainer-only record omitted from this review export)。以下は履歴。

2026-10-19 R11完了：340完了／21未完了／361項目、元25残行はR11・R12・R13・R15の4完了／21未完了。Admin通常サインイン、復旧フォーム取消し、認証保留の私有画面禁止・破棄確認Cancelで保持・破棄確定後の元Admin復帰を実nativeで確認。既存の切替／復旧成功／コード表示非表示と結合した。証拠 (maintainer-only record omitted from this review export)。全N02／正式AC／OS行列は別条件。以下は履歴。

2026-10-18 R13完了：339完了／22未完了／361項目。元25残行はR12・R13・R15の3完了／22未完了。role作成・割り当ての人間確認、native解除取消し／確定／再割り当て、role全体read許可・個別Boardの人間確認に加え、Thread限定拒否→MemberでBoard維持／Thread・本文非表示→継承復帰→同本文再表示をnative＋手元資格入力で確認。既存の同資格・開いた私有画面失効証拠と結合。証拠 (maintainer-only record omitted from this review export)。全N03／正式AC／通知OS行列をこの1行の完了と混同しない。以下の旧集計は履歴。

2026-10-18 Computer Use再開：338完了／23未完了／361項目。元25残行はR12・R15の2行完了／23行未完了。既存H1のAdminからnative操作を継続し、RULESのText／Form、不正draft拒否、確認取消し、同時編集競合・draft保持、再読込、保存中disabled、成功／失敗表示と永続結果を確認。検査ルールは元160bytes／同一hashへ復帰した。証拠 (maintainer-only record omitted from this review export)。正式26AC全体は未受入。以下のH1経過は過去記録。

2026-10-18 H1進行中：336完了／24未完了／360項目。元25残行は1行完了／24行未完了。R12の管理フォーム取消し／無効化・有効化／reset・拒否／成功・creation-pending時の変更禁止と確認後復帰を実native＋人間操作で確認し、既存資格失効証拠と結合した。β対象選定と初回skip・コード等の部分条件も解消。[実行順と25行対応表](REMAINING_WORK.md)を使う。既存の必須条件・保留・公開／データ保護・再試行上限は変更しない。
<!-- acceptance-current:end -->

- [x] 2026-10-18追加依頼：残る必須受入条件を直接減らす方針へ変更し、元25残行の実行順・前提・保留との対応を既存文書へ整理。集計・文書整合・製品／候補不変性の反復確認をCLIスクリプト化し、失敗検出11回帰・UTF-8・292source snapshot照合・37候補hashを確認。[CLI手順](DEVELOPMENT.md#acceptance-bookkeeping-cli)と実行記録 (maintainer-only record omitted from this review export)を参照。製品受入の自動PASS化・再試行上限解除・公開は行わない。

## Retained installed VM continuation — 2026-10-20

- [x] 固定VMの既存InstalledとOwnerFixed／ProbeData-Fixedを使い、実稼働中GuardRunning23でruntime／install／Data保持、認証済み停止後の両stream回収とidle0、通常NSIS self-copy uninstallのstable3確認、元Data22file全hash保持とPATH値型・所有登録解除を検査する。通常shutdown後Off／元chain／baseline／切断networkを照合。旧日付のretained候補に限る部分証拠で、通常UI／checkbox／通知／独立ACL／O02全体・正式ACは別gate。実結果 (maintainer-only record omitted from this review export)。

## β公開前の優先範囲 — 2026-10-15

- [x] 2026-10-19追加依頼：パスワード変更成功後に完了Toastを表示し、秘密非表示・失敗時の非誤表示・実画面を検証する。
- [x] 2026-10-19追加依頼：ログイン・作成成功したアカウント名を履歴として残し、名前入力欄で候補を選べるようにする。Data別暗号化・失敗名非登録・logout／再起動後の候補を検証する。

- [x] 2026-10-18追加依頼：以後可能な限りComputer UseでSwarm実アプリの残検査を実施する。隔離H1へのアプリ全操作承認（削除・復元を含む）を反映し、既存process／途中設定から再開、旧human-only方針との差を記録する。秘密・実行環境・公開・保護Data・消費済みincident予算の境界は維持する。

- [x] β先行対象をユーザー判断でWindows11 x64・WebView2導入済み／Codex＋Gemini CLIへ確定。他の既存AIの証拠・β確認未完と正式AC条件を維持。有効機能の安全性を検査してから提供し、公開・機能OFF・incident予算追加は未承認。[範囲](BETA_RELEASE_GATE.md)。R09／R20／R22／R24の選定部分のみ完了、実導入・未達受入は下の残行へ保持。
- [x] 新規H1隔離Dataの実nativeで人間が初回「スキップ」によるパスワード設定省略、復旧コード表示／非表示、誤アカウント切替拒否／取消し、Member追加・管理、Admin復旧を実施。実native／ユーザー画像で最終状態を照合。role作成／割り当ての確認取消し・確定、role全体read許可と個別Board拒否／継承復帰でMemberの可視→不可視→再表示も人間が確認。R10／R11／R13の部分証拠で、全幅・全認証・認証pending取消し・Thread固有権限等は未完了。初回通常submitの空欄拒否は仕様通りで親の案内を訂正。記録 (maintainer-only record omitted from this review export)。

- [x] ユーザー指定のLHで保持／完全削除／通知再送とrenderer向けエラー公開の限定点検を1round実行。executor／auditorとも調査範囲の具体的不具合なし・製品source変更なし。親側で監査指摘のコマンド記録・人間用trash／restore権限経路・定期backupログ経路を補完し、開始時source hashと差分・文書・候補を照合した。native／実日数／OSの受入と既存再試行上限は維持。証拠 (maintainer-only record omitted from this review export)（2026-10-17依頼の実行／親側補完部分）。
- [x] LH限定点検の補修済み証拠を独立確認。再監査run20261016T183214Z_4433e36dは2round上限でincomplete／clean／needs_revisionのまま終了したが、権限・ログ・入力5file・17結果・製品278file不変を確認。残ったコマンド来歴は親側と独立native Lunaレビューが元trajectoryへ直接照合し、17文字列／結果hash／終了コード一致をcomplete／clean／alignedとして確認した。この限定残条件だけを完了し、両LHの元判定・元の25複合受入・正式26ACは変更しない。証拠 (maintainer-only record omitted from this review export)。

2026-10-17 03:46 LH継続・独立確認結果：332checked/25open/357total。証拠の独立確認1件を完了。元の25残条件を閉じたり、製品不具合修正・native／実日数の新規受入を計上していない。追加LH起動は不要。

- [x] N04の実release候補に対するrenderer観測経路を確立。検査子processだけのWebView2診断フラグ・専用cacheでDevTools／非空ログを取得し、実資格等5canaryを4観測出力で照合して露出0。合成例外の貼付けはbrowser安全guardで拒否され、解除・実行せず、アプリ例外受入は未完了へ保持。比較Post不変・通常GUI終了・専用runtime停止・当該通知登録解除まで確認。証拠 (maintainer-only record omitted from this review export)。

2026-10-16最新継続集計：330checked/25open/355total。今回2件はruntime通知安全性とrenderer観測経路の追加証拠であり、元の25未完了行や正式ACを合格にしていない。

同日後続の実日数保持baseline準備は補助のIPC起動／API選択不備で失敗。新しい専用Dataの部分状態を保存し、最終専用runtimeは通常停止。削除・保持設定・baselineは未作成で、N07や準備完了には数えない。この準備incidentの初回＋2retryも消費済みのため第四の自動実行はしない。修正補助はcompileのみ確認し、end-to-end未確認を保持。詳細はHANDOVER (maintainer-only record omitted from this review export)冒頭と`target/renderer-observation-20261016/elapsed-seed-final.json`。

- [x] N07の通知再送と削除・再起動の結合をruntime回帰で確認。実workerの集約／再送時間、保存済み失敗batch、Service／私有bridge再openを使い、Post削除・Thread／Board完全削除後は削除対象を再送せず、生存する比較Postだけを配信先sinkへ渡す3経路を確認。通知4成功／OS検査1ignored、fmt／runtime全target Clippy成功。実経過日数・native OS通知・全N07は未完了のまま。証拠 (maintainer-only record omitted from this review export)。

2026-10-16継続集計：329checked/25open/354total。通知再送の独立したruntime安全性検査を追加完了。既存の25複合受入条件・正式AC・公開／データ保護／再試行上限を維持。

- [x] AGENT.mdにLH、Serena、CodeGraph、Luna Subagent Orchestratorの積極活用方針を明記。適用条件・総コストと調整負荷・実行予算・既存の保護／停止条件を維持し、旧ローカルモデル起動の推奨を除去。各用途と主エージェントの統合責任を明文化し、読み戻し・UTF-8・件数を確認（2026-10-16追加依頼）。

2026-10-16 22:10最新集計：328checked/25open/353total。開発ツール活用方針の追加依頼1件を完了。既存25未完了条件と実行承認・検査／公開境界は維持。

- [x] 検索だけ／直接返信一覧だけの権限拒否を表示失効へ反映。Board／Threadのsearch・Threadのresponse_readを既存人間用capabilityへ追加し、投稿読取許可の維持と各拒否をcore／私有通信／guard回帰で確認。旧実nativeで検索カード／直接返信パネル残留を各再現し、新matching候補・同一Data／資格で消去を確認。全core259・frontend395・private pipe1・fmt／Clippy成功。全N02／N03の他条件は維持。証拠 (maintainer-only record omitted from this review export)（2026-10-16必須privacy残経路）。

最新継続集計：327checked/25open/352total。検索・直接返信の独立拒否に対する新規不具合1件を追加・修正・matching native再確認済み。元の25複合条件、公開／保留・OS上限は維持。

- [x] 削除済み項目だけの権限取り消し時の表示残留を修正。全391test成功後、Board／Threadの消失・purge権限低下と単体／一括確認の最終4回帰・型も確認。旧matching nativeでService一覧0・live capability不変でも私有名残留を再現し、新matching候補・同じDataで私有名／説明／一覧の消去を確認。全N03／N07の他条件は維持。証拠 (maintainer-only record omitted from this review export)（2026-10-16必須privacy検査）。

今回の継続集計：326checked/25open/351total。資格再設定・パスワード変更・Admin復旧の実発行資格／旧native画面失効を一括確認し、削除済み一覧の別経路の残留不具合を追加・修正・native再確認。元の25複合受入条件をまとめて合格にはしない。資格の証拠 (maintainer-only record omitted from this review export)。

2026-10-16 19:53の継続完了：matching候補の専用Dataで親Board非表示・Thread個別ロール許可を実Tauri確認。設定の非公開アカウント一覧・更新履歴・ピン留めと本文をそれぞれ表示してロール所属を取り消し、旧資格で取得拒否／一覧空、nativeの私有情報消去を確認。設定取消しでは件数・未読0も表示、別2経路ではWorkspaceペイン消去を確認。再付与後の同じ資格での更新／再取得と保存Post不変も照合。既存実装行のnative証拠を補強し、実編集・通知等の全N02／N03条件と25未完了は維持、325/25/350。証拠 (maintainer-only record omitted from this review export)。

2026-10-16 19:38の最新集計：325checked/25open/350total。UI文言の追加依頼1件を完了。既存の未完了条件は維持する。

- [x] UIの固定文言にある「人間」を「ユーザー」へ置換。権限管理の案内・復元の最終確認・適用結果の3箇所を変更し、UIソース全文の残存0・厳密UTF-8・型チェック・関連3file／60test成功を確認。既存データは不変。文言のみの低影響変更としてlive/native操作・配布候補再作成は行わず、旧固定候補は変更前のまま保持（2026-10-16追加依頼）。

2026-10-16の追加集計：324checked/25open/349total。実native正常復元から実発行済み人間2資格／AI token・resume失効、人間の手動再ログイン、保存投稿ID／本文／revisionと旧cursor拒否までの接続を完了。失敗誘発2試行はいずれも予約解放後の正常適用であり、巻き戻し成功には数えない。元の25未完了は維持。証拠 (maintainer-only record omitted from this review export)。

- [x] matching候補の実Tauri正常復元、実発行済み人間2session／AI token・resumeの失効、手動再ログイン後の保存投稿ID／本文／revision復帰と旧human cursorのRESYNC_REQUIREDを同じ隔離fixtureで照合。終端applied・同一intentも確認。失敗時rollbackと全N13は未完了へ維持。証拠 (maintainer-only record omitted from this review export)。

2026-10-16の最新集計：323checked/25open/348total。N04の実GUI／runtime stdout・stderrと実編集競合の公開エラー／ログの観測部分を完了。rendererコンソール・未観測例外等の元条件は未完了のまま維持する。証拠 (maintainer-only record omitted from this review export)。以下は前の節目。

- [x] matching配布候補の実Tauri GUI／runtimeの通常操作と編集競合、二重GUI／不正引数の起動拒否で公開出力を観測し、実DPAPI資格等5canaryの非公開を照合。8stdout／stderrと、必要markerの存在を確認した4native表示で漏出なし。競合下書き保持と未commitも実GUI／private readbackで確認。rendererコンソール／未観測例外と全N04は元の未完了へ維持。証拠 (maintainer-only record omitted from this review export)。

2026-10-16午後の最新集計：322checked/25open/347total。新規に発見したbackupパス別名の重複／pending保護不一致1件を修正し、Windows回帰とmatching候補の実native再検証を完了。下の321/25/346は発見前の完了節目。

同日の追加native証拠：単体Threadと全選択3件の確認／取消し／完全削除、配下を含む4Threadの本文取得NOT_FOUND、比較用の復元済みPost不変を確認。安全コピー284.6KiB→0Bと手動／定期backupの個別完全削除を実行し、実ファイル・receipt・準備記録消去と現在の会話／認証の保持を照合。保持日数1／2／3の保存・再取得後に無期限へ戻した。既存4実装行のnative証拠を補強し、期限経過／通知等の全N07と正式ACは残す。証拠 (maintainer-only record omitted from this review export)。

同候補のnative完全停止・復帰、CLI停止／稼働状態、停止中の保存／整理操作無効、復帰後の削除済み本文・backup・準備記録の非復活も確認。全N10のOS条件は未完了。検査GUI／runtime／通知登録を片付け、production52064のみを保持。最終runtime82件＋子worker2件・失敗0、private pipe1件、fmt／Clippy／UTF8・候補37hash照合成功。件数322checked/25open/347totalを再照合。

現在の集計：321checked/25open/346total。今回追加4行を完了。frontend388件＋最新整理画面4件、全Rust565件／子worker3件・失敗0、fmt／Clippy／UTF-8成功。36fileローカル候補candidate-20261016-trashとNSISを固定。実native不可逆操作・元の正式受入・保留条件は別途維持。証拠 (maintainer-only record omitted from this review export)。

2026-10-16継続検査：同じ候補の実Tauri・新規隔離Dataで全6件選択／解除、親Board＋明示削除した子Thread2件の一括復元確認・取消し・実行を確認。元のPost ID／本文一致、未選択の兄弟Threadは削除状態を維持、復元本文の実表示と既読反映も確認。保持5項目の無期限初期値と内部スクロールも確認。既存完了行のnative証拠を補強したもので、件数は321checked/25open/346totalを維持。不可逆削除・定期消去の実native受入は未完了。追記証拠 (maintainer-only record omitted from this review export)。

2026-10-16追加依頼の検証前集計：317checked/29open/346total。下記4行は今回追加した機能条件で、過去の317checked/25open/342totalとは分ける。

- [x] 実native検査で発見したbackup保存先の通常／拡張長パス別名による重複一覧とpending保護・receipt整理の不一致を修正。実Windows別名で一覧1件、手動／定期pending削除拒否、完了後のfile／receipt整理を回帰検証。runtime全件・private pipe・Clippy／fmt／matchingWindows候補成功、同じ隔離Dataの実画面で4行→2件と確認／取消しを確認。証拠 (maintainer-only record omitted from this review export)。

- [x] 削除済みBoard／Threadの単体完全削除を人間用Service／private GUIへ実装。配下の投稿・履歴・関連状態を除去し、復元不可・権限再確認・AI拒否・既存Data移行を検証。core trash7件、management5件、migration2件と実private pipe成功。native不可逆操作の受入とは分ける。証拠 (maintainer-only record omitted from this review export)（2026-10-16追加依頼）。
- [x] 削除済み一覧へ「選択」・checkbox・全選択と、選択／一括復元・完全削除を追加。単体「復元」の横にも完全削除を置き、対象と不可逆性の確認・取消し・権限／競合エラー・狭幅を検証。144focused DOM／型、実private pipe一括復元・削除、英light／日dark560pxの実ブラウザ表示と取消し・一括復元を確認。native不可逆操作の受入とは分ける。証拠 (maintainer-only record omitted from this review export)（2026-10-16追加依頼）。
- [x] データ保持へ削除済みBoard日数・削除済みThread日数・過去スレ日数を追加。既存Dataは新項目無期限で移行し、期限到達時の自動完全削除・関連削除・停止条件を検証。retention unit6／integration2、旧7・8／新9backup互換、runtime停止・復旧gateとprivate pipe旧caller設定維持を確認。証拠 (maintainer-only record omitted from this review export)（2026-10-16追加依頼）。
- [x] 蓄積データの削除経路を棚卸しし、会話・履歴・変更／通知／idempotency・キャッシュ・backup等の整理設計と容量回収を確認・記録。単体／定期purgeのSQLite収縮・busy reader時の永続再試行、参照のない期限切れAI情報整理、backup個別削除・復元artifact明示破棄と復旧gateを実装・検証。通常設定／資格はData全廃棄、別保管コピー／RULES回復コピーは保存先の明示削除経路を案内。OS全廃棄の実行受入は未実施。[保存先一覧](STORAGE_LIFECYCLE.md)・証拠 (maintainer-only record omitted from this review export)（2026-10-16追加依頼）。

- [x] N07の直接返信0／98／99／100件を新規専用fixtureの実Tauriと公開CLIで照合。0は非表示、98は実数、99と100は99+、CLIは上限で丸めない。99件の直接返信一覧を実操作で追加読込し、全99件／追加読込終了を確認。削除・保持／通知のN07全条件は元の未完了へ保持。証拠 (maintainer-only record omitted from this review export)（2026-10-16）。

- [x] 開いた設定の管理／権限／RULES capabilityを可視時5秒・focus／復帰時に再確認し、権限減少でモーダルを閉じ私有一覧・本文・確認内容を破棄。遅延・権限追加・通信失敗・account切替・scope減少の回帰を含む全frontend378件／型／matchingWindows候補成功。実旧GUIの一覧残留を再現し、修正版では同じ資格のService拒否とモーダル／管理者名消去を確認。権限／RULESの全native変更行列は元の残条件へ維持。証拠 (maintainer-only record omitted from this review export)（2026-10-16）。

- [x] 実Tauriの公開ログを展開し、初期読込・Thread選択・現Thread更新・ログ内スクロールの表示を確認。実DPAPI保存の人間token／recovery code・transport capability・合成passwordの4canaryを4観測surfaceと照合し露出なし。前回の展開未確認を補完し、renderer console・異常経路とN04全体は未完了に維持。証拠 (maintainer-only record omitted from this review export)（2026-10-16）。

- [x] 復元成功／DB書換え直後の失敗／資格消去直後の失敗による巻き戻しで、実発行した人間2session・AI session／resume secretと変更cursorの失効を実Named Pipeで検証。既存bridgeと再open後の旧資格拒否、再認証後の旧cursor拒否、保存内容の成功／rollback差、完了再照会後も新資格／cursorが有効なことを確認。直接SQLite fixture・製品failure APIを追加せず、隔離workerに90秒上限。N13の実Tauri・強制終了／OS障害等は別途未完了に維持。証拠 (maintainer-only record omitted from this review export)（2026-10-16）。

  2026-10-21補強：実Tauri／固定配布候補のpost-active file競合→自動drain→正常再起動rollback、旧人間alias／cursor拒否と元2Post保持・native再表示を確認。手動native再認証・同fixtureの人間2／AI全行列は未受入。追加証拠 (maintainer-only record omitted from this review export)。

- [x] N06の実GUI／公開CLI共有状態を新規fixtureで受入確認。交互のPost／Response作成・編集、両方向の競合拒否・下書き保持、同一ID／所有者／revision／履歴、GUI再起動後の保存、No5削除後No6非再利用と同ID対象への返信を照合。指定削除は人間APIのfixture操作でnative確認とは分離し、広い起動停止／全client／N07条件は未完了へ保持。証拠 (maintainer-only record omitted from this review export)（2026-10-16）。

- [x] 設定「プロジェクト」に共通RULES／任意の個別RULES／AI共通指示と登録・session紐付けの開閉式詳細ガイドを追加。「権限・ルール」に自然言語でAIへ下書きを依頼し、内容・適用先を確認して検証／保存する案内を追加。日英／両テーマ・420／360px、開閉・スクロール・既存tab操作、frontend365tests／型／build成功。matching Windows／NSIS候補と実GUI20748の日本語lightで両案内・詳細展開／スクロールも確認。自動AI機能や権限変更は追加しない。証拠 (maintainer-only record omitted from this review export)（2026-10-16依頼）。

- [x] 全体検査とWindows候補生成へ共通のfrontend-build排他を追加。競合するcheckの実プロセス拒否・解放後再取得を検証し、Tauriのdist全体監視／custom-protocol index必須検査で空資産cacheも再生成する。失敗候補を保存し、直列再生成したassets-fixed候補の実GUI43096で正常な画面・右寄せtoolbarを確認。証拠 (maintainer-only record omitted from this review export)。

- [x] 右カラム上部の設定などのボタン群を右寄せにした。既存Grid／幅／gap／760px境界を維持し、1200／900／761pxで行の右端整列、760／560／360pxで変更前と同じ位置・幅・折り返しを確認。英語light／日本語dark、設定・検索・pane resizeも操作確認。実native受入とは分離。証拠 (maintainer-only record omitted from this review export)（2026-10-15追加依頼）。

- [x] 検索結果・別Thread・履歴の表示中も、Serviceの本文閲覧権限post_read／既存capabilityの減少を再確認し、私有表示と検索ダイアログ・古い検索遷移を失効させる。全365frontend／552Rust成功、資産修正版の実GUIで「Thread未選択の検索結果→本文のみ拒否→消去」と「履歴表示→履歴のみ拒否→消去」を確認。新規grant・通信失敗・遅延応答の回帰を保ち、元のN02／N03／N04全条件は別途残す。証拠 (maintainer-only record omitted from this review export)。

- [x] 別人間セッションで開いているThreadの読取を拒否／資格を失効したときに旧GUIへ本文が残る不具合を修正。本文を取得しない可視時5秒／focus確認で私有Workspaceを消去し、資格失効時は認証画面へ戻す。全frontend361tests、非可視・遅延応答／Thread切替回帰、matching候補の実GUIで本文消去と再認証画面を確認。全N02／N03／N04の受入は別途残る。証拠 (maintainer-only record omitted from this review export)。

- [x] ユーザー指定の公開前必須範囲を優先し、後回し候補を当面保留として記録する。必須の安全性と条件付き保留を分け、保留を合格・機能OFF・公開承認とは扱わない。[β判定の範囲](BETA_RELEASE_GATE.md)。既存26未完了行の条件は保持し、優先順位を本節で上書きする。

Current321checked/25open/346total. Four additional trash/retention/storage implementation rows are complete. Original native delete/restore dialog/list/parent-child row completed; N07 direct-response boundaries also accepted. Required safety checks are active; exhaustive optional/environment matrices are held subject to BETA_RELEASE_GATE. This priority change neither clears formal ACs nor authorizes publication.

## New store under Modify-only parent — 2026-10-15

Current304checked/27open/331total. Normal-host causal repair; elevated VM/native acceptance remains separate.

- [x] 新規Data配下のstore初期化が、親からModifyのみを継承する通常ユーザーでも成功し、新規対象の本人所有／本人専用非継承ACLと既存対象の検査・拒否を維持する。実ACL回帰の修正前失敗／修正後成功、通常platform54成功／2ignored、N09専用fixtureの初期化成功を確認。既存不正storeの修復やVM再試行なし。記録 (maintainer-only record omitted from this review export)。

## Retained-install harness recovery — 2026-10-15

Current303checked/27open/330total. Local integration only; guest continuation remains NOT_RUN.

- [x] 既存Installed／ProbeData-Fixed／失敗receiptを保持する継続検査へretained Process handle・Refreshなしの終了コード取得と認証停止後のstream回収を統合し、Windows PowerShell5.1で速い終了・失敗・両stream・停止前回収拒否を検証する。同一OS incidentの初回＋2retry上限は消費済みのまま、VM再実行やO02合格とは扱わない。検証・レビュー (maintainer-only record omitted from this review export)。

## Elevated Windows private-store ownership — 2026-10-15

Current302checked/27open/329total. Confirmed elevated-owner defect repaired and scoped runtime/backup verification completed; independent elevated file/lock matrix and existing OS/native residuals remain open.

- [x] 管理者tokenが新規作成したstore・credential／backup一時file・lockに本人所有を明示し、本人専用非継承ACL検査と既存不正対象の拒否を維持する。通常Windowsのplatform53成功／2ignored・lockを含むACL回帰／全workspace／Clippy／matching候補成功。管理者VMではruntime起動・合成資格保存と再読込・手動backup成功、4storeの本人所有／保護をread-onlyで確認し、旧誤所有storeは変更せず拒否。下の独立管理者file／lock行列は残す。記録 (maintainer-only record omitted from this review export)。
- [x] 管理者VMで新規ciphertext／backup file／CredentialStore lockのowner・ACE・非継承を独立に直接照合し、lock再取得・通常の人間用停止まで確認する。2026-10-20に実elevated guestで新規5対象を別PowerShellから2holder計10観測、競合待機／解放／再取得・元Data22file保持を検証。18:30の認証済みhuman-session正常停止・両stream／idle0の実結果を別version文脈として明記して結合し、旧4store metadataや通常権限回帰の代用ではなく直接行列を完了。独立検査と境界 (maintainer-only record omitted from this review export)。

## Native restore safety preparation — 2026-10-15

Current301checked/26open/327total. Fresh isolated native Safety preparation/cancellation and successful apply/manual re-login completed; failure rollback/credential-cursor matrix/OS cases and full N13/AC-021 remain open.

- [x] 元Data／project RULESへ触れない専用fixtureで、実Tauriのbackup選択・両RULES復元先確認・Safety保存完了・最終確認から戻る操作と未適用の会話保持を検査する。通常GUI終了後のprivate APIでもprepared／同じSafety／変更後2投稿の保持を確認。復元適用・再認証・失敗時rollback等は既存の未完了項目へ残す。記録 (maintainer-only record omitted from this review export)。
- [x] 専用fixtureの実TauriでSafety準備の通常GUI再起動保持、実行時承認後の復元適用、再ログイン要求と人間によるサインイン、その後No.1の保存本文／No.2不在を確認。read-only DBでも同ID・revision1・整合性／FK・適用前human_sessions0・両RULES／Safety保持、製品inspectorで入力とSafety2件を照合。失敗時rollback・AI資格／cursor行列・OS障害は残す。記録 (maintainer-only record omitted from this review export)。

## Narrow header wrapping — 2026-10-14

Current299checked/26open/325total. Header implementation/browser validation completed; rebuilt native candidate is separate from unverified native acceptance.

- [x] 上部Toolbarの通常幅ではボタン群／折り返した行を左カラムの右端へ揃える。狭幅ではロゴ下の余白も使って左側から並べ、左右同じpadding内で収まらない時だけ折り返す。日英・両テーマ・6幅のbrowser描画／操作、左カラム幅+16pxへの追従と復帰、8px gap維持を確認。frontend353test／型check／matching staticCRT・NSIS／35file private候補成功。実nativeの新header受入は画面復元後に残す。記録 (maintainer-only record omitted from this review export)。

## Batched disposable-Windows continuation — 2026-10-14

Current298checked/26open/324total. Branch-specific VM diagnosis and ten actual installed checks completed; PATH and update/uninstall retention rows closed. Full O02/native/formal/publication gates remain.

- [x] 使い捨てVMの旧検査先Installed／結果file／既定Dataを個別に読取確認し、前回の停止が既存Data分岐によることを特定。guest identity／policy／Swarm process・登録absenceを確認し、既存Data保持の新規検査先へ切り分けた。記録 (maintainer-only record omitted from this review export)。
- [x] 使い捨てWindowsの実候補で35installed hash、所有PATH項目だけの解除・他項目／型保持、NSIS再登録、同版更新のData保持／重複なし、AddPath反復／Remove・再追加、installed CLI起動、実engine完了後のPATH／所有記録・HKCU解除とData22file全hash保持をまとめて確認。10検査PASS、正常VM終了Off。NSIS公式_?=待機方式を使用し、通常UI／checkbox／起動中拒否・通知を含むO02全契約は残す。記録 (maintainer-only record omitted from this review export)。

## Current candidate native continuation — 2026-10-14

Current294checked/28open/322total. Matching candidate/source snapshot and native backup-selection partial verified; full native/formal/OS/publication gates unchanged.

- [x] 最新source候補の実TauriでN13の保存先フォルダー初期表示／.backup filter／既存manual archive選択・metadata／RULES復元先／再選択取消しの選択保持／選択状態解除を確認。Safety準備・適用・巻き戻しは実施せずN13全契約を残す。記録 (maintainer-only record omitted from this review export)。

- [x] 更新確認を既存GUI hostのHTTP依存／TauriのSemVer型へ整理し、追加の直接依存によるCargo.lock／同梱notice driftを解消。元lock hash一致・9resources／5MPL sources・manifest境界test・Clippy・frontend353test・matching release／NSIS成功。記録 (maintainer-only record omitted from this review export)。

## Backup location and startup update notice — 2026-10-14

- [x] ブラウザプレビューのバックアップ選択ボタンからファイル選択ダイアログを開くよう修正。キャンセルで既存選択維持、内容読取／外部送信なし。8関連test／browser filechooserイベント・検査専用ファイル名表示確認。ブラウザ初期フォルダー制約とnative保存先指定を区別。記録 (maintainer-only record omitted from this review export)。

Current292checked/28open/320total. Preview chooser correction verified; native picker rendering acceptance remains separate.

- [x] 「バックアップファイルを選択」の初期フォルダーを保存済み設定／未指定時Data/backupsへ合わせる。実Pipeの既定・変更先・account変更拒否を検査。native表示受入はN13に残す。記録 (maintainer-only record omitted from this review export)。
- [x] 仮URLで起動時の最新版を確認し、赤字報告・バージョン・更新ボタンを既存Quick Startの下へ表示。確認失敗時は利用継続。ユーザー指定により現在は仮URL／UIの範囲。記録 (maintainer-only record omitted from this review export)。
- [x] 更新ボタン下のProgress Barと検証／インストール／完了・再起動表示、二重実行抑止／失敗時再試行を実装。プレビューの進捗→ページ再読込を確認。実更新成功と区別。記録 (maintainer-only record omitted from this review export)。
- [x] 更新直後の起動中だけ右カラムにアップデート内容／更新済みバージョンを表示するUIを実装。既存Empty Stateを隠し上端／90%幅／大きい文字、次回通常起動で元のEmpty Stateへ戻す。プレビューの起動時1回受取／次回再読込で復帰。記録 (maintainer-only record omitted from this review export)。
- [ ] 実GitHub配布→ダウンロード／インストール／再起動→更新内容の起動時1回表示・次回起動時復帰を使い捨てWindowsで受入検査する。2026-10-24の追加指示で固定配信URL・バージョン署名検証・認証済みruntime停止・設定保存・NSIS更新実行／再起動・保護receipt永続化を実装。単体／画面sample／ローカル署名と本配布の受入は区別する。[更新ガイド](UPDATES.md)。公開保留中に旧検査を反復しない。

Current291checked/28open/319total. Placeholder UI is complete; native signed delivery remains a publication-phase residual.

## Candidate installer snapshot repair — 2026-10-14

- [x] 候補reportのinstaller参照を後続buildで上書きされない候補内コピーへ固定。copy/hash一致・後続source変更／削除後の保持・copy中変更拒否・既存候補非上書きの10test、fresh35file実候補＋独立installer照合成功。installedファイル契約・公開保留維持。記録 (maintainer-only record omitted from this review export)。

Current287checked/27open/314total. Candidate installer snapshots completed; formal/native acceptance remains separate.

## Bounded large-page rendering repair — 2026-10-14

- [x] 50件の長文有無比較と500／1000件の画面外layout抑止を限定試験し採否を判断。最下部363px不足を検出し試験内で修正、browser/nativeの最下部／再開位置／キーボード・native500件を確認したが、native検索No53が隣の長文へずれる副作用とGUI1,016,975,360 bytesの512MiB未達により不採用。試験変更を撤回し、既存Markdown memoを維持する。N14／AC-026性能・表示の全契約は従来の未完了行で継続管理。記録 (maintainer-only record omitted from this review export)。

Current286checked/27open/313total. Bounded experiment rejected; matching reverted candidate35hash/Data/PATH update and native highlightedNo53/depth16 search readback verified. Full visual/performance acceptance stays open. CurrentGUI41800/window4923102/runtime77472, same isolated Data. Trial builds/hashes historical. Earlier summaries below historical.

## Large native workload measurement — 2026-10-14 09:26 JST

- [x] N14で判明した5秒ごとの未読／購読更新に伴う不変Markdown再解析を抑止し、本文／言語／リンクcallback変更は反映する。型／frontend346件成功、matching release／NSIS・隔離35hash更新／Data・PATH保持、1000件再測定完了。GUI1,174,564,864 bytesで512MiB未達のため性能全契約は未完了を維持。記録 (maintainer-only record omitted from this review export)。

Current285checked/27open/312total as of2026-10-14 09:35JST. Markdown polling parse suppression implemented/tested/installed; full memory gate still FAIL. Current GUI73156/window5313954/runtime61648; same isolated Data, source candidate390files. Earlier observations below historical.

- [x] 最新installed候補・既存1000件fixtureで事前目標を定め、並列8の読取／検索40件とnative1000件表示の時間・GUI子process合算／runtimeメモリを測定する。40件成功、全体0.375秒／p95 85.16ms。GUIメモリは512MiB目標超過、画像のみの再試験でも1,299,447,808 bytes。測定完了のみを記録し、性能合格・N14／AC-026完了にはしない。50件／page20へ復帰済み。記録 (maintainer-only record omitted from this review export)。2026-10-22はliteral段落だけ同一HTML直接renderを実装し全frontend402tests／typecheck／release成功。新隔離実native1000件の対比較1354543104→880955392bytes、35%減だが512MiB未達。全文／CSS不変、両Data保持・正常終了、全R10は未完了。追加証拠 (maintainer-only record omitted from this review export)。

Current284checked/27open/311total. N07 parent-only deletion/restoration complete, No1/2/3 revisions4/3/1. N14 memory target FAIL; accessibility-text-only cause rejected by fresh screenshot-only reproduction, allocation cause unresolved. Full acceptance/publication/exclusion boundaries retained. Older summaries below historical.

## Approved parent deletion continuation — 2026-10-14

- [x] 準備済みの検査専用No.1削除に対する実行時承認を受け、nativeで親だけ削除・復元。子ResponseNo.2／独立PostNo.3がID／revision／本文／author／関係まで不変、親本文検索0→1、親同ID／本文復元を公開CLI各6確認と実画面で検証。No.1現在revision4、No.2は3、No.3は1。保持期限・親Board／Thread・件数matrix等はN07残条件。記録 (maintainer-only record omitted from this review export)。

Current283checked/27open/310total. Parent restore completed; no pending deletion approval for this No.1 cycle. Broad future authorization retains mandatory per-action confirmations and authentication/privacy denies, exclusions/publication hold.

## Batched candidate dependency repair — 2026-10-14

- [x] 最新検査候補を妨げる同梱AI文書の履歴証拠リンク17件をsource locatorへ整理する。同梱19文書のリンクpreflight、matching release／NSIS build、35fileのfresh private stageと389fileのsource exportを確認。既存の隔離hostへ更新し、35installed hash・Data／PATH保持を照合。private履歴の同梱・公開・production導入は行わない。記録 (maintainer-only record omitted from this review export)。
- [x] 最新候補のnative完全停止・AI CLIのRUNTIME_STOPPED拒否・明示GUI起動による復帰・GUI通常終了後runtime／CLI継続・保存済みPost／Responseの保持を検査する。同boot／次bootの自動起動、全transaction drainなどN10／O03の残条件は保持。記録 (maintainer-only record omitted from this review export)。

Current282checked/27open/309total. Batched dependency repair/matching isolated update/native lifecycle partial complete. Manual archive validated through product code including malformed-input refusals. Parent No.1 deletion confirmation prepared, not executed; fresh read-only baseline/release PASS and action-time approval pending. Native/formal full gates/publication hold/excluded clients preserved.

## Native author and backup continuation — 2026-10-14

- [x] 実Swarmで人間Post／Responseと既存AI Postの「不明: 不明」fallbackを確認する。保存済みNo.1000までの旧snapshotを変更せず観察。既知model／harnessの新規snapshot・補完後の過去表示はN09の残条件として保持。記録 (maintainer-only record omitted from this review export)。
- [x] 隔離native GUIで手動バックアップを作成し、完了表示・結果確認済み操作・生成ファイル／hash／既存定期2世代保持を照合。native復元file pickerの起動・取消しを確認する。file選択・準備・適用／巻き戻し・再認証などN13全体は別gate。記録 (maintainer-only record omitted from this review export)。

Current280checked/27open/307total. N09/N13 partial evidence added, only N08 full native PASS;24native full groups/26formal ACs remain NOT_RUN. Refresh-button tests USER_CONFIRMED, publication held/excluded clients unchanged. Earlier totals historical.

## Scoped refresh controls — 2026-10-14

User reports all refresh-button tests complete with no issues (2026-10-14). The two implementation rows below are now additionally USER_CONFIRMED; this does not close unrelated formal acceptance.

- [x] 各更新ボタンの実行中は「更新中」、成功時は「更新完了」のトーストを表示する。失敗は「更新に失敗しました」と表示し、空対象の既存案内を保持する。共通トーストへ接続、二重実行抑止・型チェック成功。操作確認はユーザー担当。記録 (maintainer-only record omitted from this review export)。

Current278checked/27open/305total. Refresh progress/completion toast implementation and typecheck complete; behavior verification remains assigned to the user. Earlier counts historical.

- [x] ヘッダーの更新を「現スレッドを更新」「全スレッドを更新」「ピン留めを更新」に分割し、それぞれ対象範囲のみ再読込する。対象なしは文字色グレーのまま操作でき、指定のトーストを表示する。型チェック成功、内部ブラウザにプレビューを表示・保持。ユーザー指示により動作確認はユーザーが行う。実装記録 (maintainer-only record omitted from this review export)。

Current277checked/27open/304total. Scoped refresh implementation/typecheck/preview handoff completed; user owns behavior verification, no agent interaction tests or installed-native update. Earlier counts below historical; full native/formal acceptance and publication hold remain.

## Native Computer Use resumption — 2026-10-14

- [x] Action-time承認された検査専用Response No.2だけを実Tauriで削除・復元する。削除中のtombstone／本文非公開／検索0件／直接返信0件、新規No.3への採番非再利用、復元後の同ID／本文／所有者／返信先／検索1件／直接返信1件とNo.3保持を実画面・公開CLIで確認。N06の競合、N07の保持期限・親子独立削除・件数98／99／100・通知などは残す（2026-10-14、記録 (maintainer-only record omitted from this review export)）。

- [x] 検査専用Threadの実Tauri作成・同一人間IDへのResponse・Post編集・人間履歴を実操作し、公開CLIで同じID／No.／revision／所有者を照合する。GUI通常終了中と再起動後の編集・返信保存、runtime／CLI継続、未許可AI編集／history拒否も確認。N06のnative／CLI交互write・競合など残条件と正式AC全体は別gate。削除後採番は上の別検査で確認済み（2026-10-14、記録 (maintainer-only record omitted from this review export)）。

- [x] Resume authorized native Computer Use ROADMAP verification in the existing isolated Swarm fixture, recording exact installed GUI hash/root/window and real link confirmation/cancel/browser-launch evidence. User manually confirmed https://example.com/ after Computer Use's browser URL-policy stop and requested continuation. Excluded client tests/publication hold/guest-only hazardous OS and individual action-time boundaries retained. Evidence (maintainer-only record omitted from this review export).

Current276checked/27open/303total. N08 scoped link acceptance completed with native Swarm evidence and human browser URL confirmation; N06/N07 have the partial native evidence above, remaining24native groups/26formal ACs remain NOT_RUN for their full contracts. Native Computer Use hold lifted by explicit user authorization; publication/distribution still held. Older counts/holds below historical.

## Client test scope decision — 2026-10-14

- [x] Record the user's permanent exclusion of Antigravity, Cursor Agent/CLI, OpenClaw and OpenCode from developer-run client tests. Removed them from pending/retry/pre-distribution client acceptance conditions, retained existing evidence/adapters, and assigned compatibility observations to voluntary post-distribution user reports in HANDOVER (maintainer-only record omitted from this review export), specification, ledger and acceptance/checklist. This is an exclusion, not a PASS; native Computer Use/publication holds remain.

Current272checked/28held-open/300total. The checked row records the user decision; excluded clients are not four additional PASS results. Remaining client/native/OS/formal/publication conditions are preserved. Earlier counts and excluded-client retry instructions below are historical and superseded.

## Same-session responding native Codex transition — 2026-10-14

- [x] Verify a genuine responding model transition in the exact previously successful native Codex test session, preserving its cwd. gpt-5.5→gpt-5.6-luna actual terminal17checks PASS for known-change refusal, unchanged old ID/deadline/Post, new automatic selection/ID/Post, ownership/scope/selector/root denial, protected resume rotation/obsolete credential refusal and independent release. Evidence (maintainer-only record omitted from this review export). An intermediate gpt-6.1-sol selection failed400unsupported before response and is not successful transition evidence.

Current271checked/28held-open/299total. Native CLI model/list differs from Desktop cache; native-advertised Luna responded in19.9seconds under240-second budget. Full all-client/other contexts/native/formal gates remain open. Native Computer Use/publication holds and26formal/25native NOT_RUN unchanged. Earlier counts below historical.

## Fresh native Codex report adoption — 2026-10-14

- [x] Verify a fresh actual native Codex session's own report through its real terminal, separate from the dated saved-record supervisor check. Exact thread.started selector/cwd/current turn metadata and16public CLI checks PASS for automatic gpt-5.5/Codex snapshots, unknown enrichment retaining ID/deadline/historical Post, scope/selector/root refusals, resume rotation/obsolete credential refusal and release. Evidence (maintainer-only record omitted from this review export). Model transitions and full all-client/native/formal acceptance remain open.

Current270checked/28held-open/298total. OpenCode's invocation-only alternate existing gemma3 cloud route returned410retired before terminal execution; it is not a successful transition. Both new fixtures stopped; native Computer Use/publication holds and26formal/25native NOT_RUN maintained. Earlier counts below historical.

## Requested new-chat handover — 2026-10-14

- [x] Prepare canonical next-chat instructions and one exact ai-memory envelope for this requested switch. default/antigravity scope verified; envelope01a13643-d110-7d72-af7e-545392f4b0b2 listed open with exact workspace cwd, disposable test envelopes preserved. Completed CLI/model/export evidence, residual acceptance/holds, owners and stale candidates retained in HANDOVER (maintainer-only record omitted from this review export). No automatic chat launch/SessionEnd/Sleep; next-chat delivery unobserved.

Current requested-switch closeout269checked/28held-open/297total.26formal/25native NOT_RUN and native/publication holds maintained. Earlier counts below historical.

## Native Codex saved-report residual — 2026-10-14

- [x] Verify the actual saved report from the exact previously successful native Codex test session, without repeating its client matrix or restarting jobs. Metadata-only id/cwd/model capture binds dated producer/root;12new supervisor public CLI checks PASS for automatic gpt-5.5/Codex snapshots, exact ID/deadline retention, refusals/resume/release. Evidence (maintainer-only record omitted from this review export). This is dated-producer report-path evidence, not new/current native client execution or a model transition; aggregate gates retained.

Current verified residual batch268checked/28held-open/296total. OpenCode native-format source repair/actual-client adoption and dated native Codex report path evidenced; full all-client/native/OS/formal/publication conditions remain open.26formal/25native NOT_RUN/native/publication holds unchanged. Earlier counts below historical.

## Actual OpenCode export compatibility — 2026-10-14

- [x] Close the actual installed OpenCode report-format dependency for client adoption: native legacy info.directory/messages.info supported alongside v2 without conversion/invented metadata. Exact selector/cwd, mixed/changed/redacted context refusal, latest unknown semantics,1MiB bound and no body transfer verified. Regression/private pipe/documentation20tests PASS; supervisor21checks and actual same-native-session OpenCode terminal21checks PASS for report enrichment/ID-deadline retention/Post snapshots/refusals/resume/release. Evidence (maintainer-only record omitted from this review export). Qwen subscription, other clients/model transitions and native/formal/publication gates retained.

Current compatibility milestone267checked/28held-open/295total. Actual installed OpenCode report path is verified; aggregate all-client/native/OS/formal/publication conditions remain open.26formal/25native NOT_RUN and native/publication holds unchanged. Earlier counts below historical.

## Responding same-session model transition — 2026-10-14

- [x] Resume the same actual Gemini native session with a different currently listed model and verify a successful changed-model response/terminal invocation. Genuine flash→flash-lite hooks/stream match selector/cwd;14real-client CLI checks PASS for known replacement refusal, old ID/deadline/Post preservation, new ID/Post model snapshot, ownership denial, resume rotation/old-session refusal and release isolation. Evidence (maintainer-only record omitted from this review export). This completes the scoped responding transition; all-client/native/formal conditions remain in aggregate rows.

Current responding-transition milestone266checked/28held-open/294total. Same-session Gemini transition and previous concurrent pair are evidenced; other clients/expanded acceptance remain open. Pro-specific404/429 remain separate provider limits.26formal/25native NOT_RUN/native/publication holds unchanged. Earlier counts below historical.

## CLI concurrent-session milestone — 2026-10-14

- [x] Actual same-native-session Gemini changed-selection reports: supervisor public CLI14checks verify replacement refusal, unchanged old ID/deadline/Post, new ID/Post selection, ownership denial, resume rotation/obsolete-session refusal and release isolation. Changed inference failed404/429 before real client operations; responding transition/aggregate acceptance remain open. Evidence (maintainer-only record omitted from this review export).

Current verified closeout265checked/28held-open/293total. Concurrent actual-client scope complete; responding transition awaits a permitted available second model/quota route. All26formal/25native NOT_RUN and native/publication holds unchanged. Concurrency-only summary below is historical.

- [x] Two independently running real clients (Gemini2.5Flash and invocation-only OpenCode/existing Ollama cloud) shared one fresh developer fixture with distinct aliases and actor/Thread-limited grants. Concurrent reads/writes, request-ID separation, peer ownership/scope denials, replies, resume/release isolation and final counts/nonmutation passed24checks. Private receipt: target/client-adoption-20261014/Shared-retry1/concurrency-summary.json. Full all-client/native/formal acceptance remains in the existing aggregate rows. Initial trial stopped on an incorrect resume-expiry test expectation; corrected against the established successful-operation renewal contract.

Current concurrency milestone264checked/28held-open/292total. Same-session model-transition acceptance is still pending: Gemini2.5Pro selection produced404 before operations; cause-specific follow-up is isolated from this completed concurrent matrix. Native Computer Use/publication holds and26formal/25native NOT_RUN unchanged.

## Additional request — 2026-10-13 CLI milestone handover

- [x] Prepare the explicitly requested next-chat handover after CLI recovery: exact objective/acceptance/holds, next bounded multi-client/model-transition work, failed-provider/export boundaries, active owners and evidence recorded in HANDOVER (maintainer-only record omitted from this review export). Exact ai-memory envelope01a13568-5d22-7970-bd9e-ccdf63557b58 verified open with workspace cwd; no other test handoff consumed. No native/publication resumption, Sleep or unrelated Git mutation.

Current requested-switch closeout263checked/28held-open/291total. All26formal/25native NOT_RUN and native/publication holds unchanged; next-chat delivery unobserved. Earlier counts below are historical.

## Additional request — 2026-10-13 Gemini/OpenCode recovery

- [x] OpenCode's configured www hostname fails DNS; official apex router reaches402subscription boundary. Backed up and repaired only the hostname. A scoped existing-Ollama cloud provider then produced a short real response and35isolated OpenCode CLI checks PASS; global provider/model selection preserved. Evidence: target/client-adoption-20261013/Client-opencode-recovery-ollama/driver-report.json.

- [x] Cause-specific Gemini invocation using actual gemini-2.5-flash produced a short response and then35isolated real CLI checks PASS; actual hook/session captured, fixture stopped. Global Gemini model/auth settings preserved. Evidence: target/client-adoption-20261013/Client-gemini-recovery-25/driver-report.json.

- [x] Investigate the actual Gemini503/OpenCode timeout causes and attempt bounded cause-specific recovery under the user's explicit operation approval. Gemini2.5Flash and invocation-only OpenCode/Ollama cloud route each35CLI checks PASS after short responses. Report/lifecycle10checks PASS separately; original Qwen402subscription and OpenCode legacy/v2 metadata boundaries retained. Only proven DNS hostname repaired with backup; production/native/publication holds preserved. Evidence (maintainer-only record omitted from this review export).

Current recovery262checked/28held-open/290total;70driver assertions and10report/lifecycle assertions PASS. All26formal ACs/25native groups NOT_RUN. Earlier counts below are historical.

## Additional request — 2026-10-13 all available CLI clients and browser preview

Current CLI batch21:52JST:259checked/28held-open/287total. Six execution subjects passed210CLI assertions; four actual report subjects passed20separate assertions. Gemini503/OpenCode timeouts remain unverified adoption. All26formal ACs/25native groups NOT_RUN; native/publication holds unchanged. Batch evidence (maintainer-only record omitted from this review export). Earlier counts below are historical.

- [x] Native Codex CLI executed the fresh isolated35check matrix successfully, with exact run-path binding and this host's existing normal execution context. Record account/client model compatibility and earlier unsuccessful attempts separately; no global configuration change. Evidence (maintainer-only record omitted from this review export).
- [x] Real Hermes Agent session invoked the isolated public CLI matrix:35checks PASS including read/search/paging/write/retry/ownership/conflict/refusal/release. Exact session20261013_211324_1e1b1a export captured; subsequent report-adapter5checks PASS separately. Evidence (maintainer-only record omitted from this review export).
- [x] Real Copilot CLI session58b227f9-0bd1-4ee7-b450-c5bf51768ec8 invoked the isolated matrix:35checks PASS after correcting the test-only permission kind to shell and restricting exposed tools. Model response was available throughout; earlier command denial was a harness permission error. Evidence: target/client-adoption-20261013/Client-copilot-retry4/driver-report.json.
- [x] Real Claude Code session383e42d7-99ae-45a2-8a39-bc4145dff418 invoked the isolated matrix after the user's credit replenishment:35checks PASS with CLI max-budget-usd0.20. Evidence: target/client-adoption-20261013/Client-claude-retry-credit/driver-report.json.
- [x] Existing llama.cpp local GGUF and existing Ollama cloud route each completed2actual model/tool-continuation responses plus35real CLI checks through bounded LangGraph harnesses. Distinguish local weights from cloud routing; preserve the existing Ollama owner. Evidence: target/client-adoption-20261013/Client-llamacpp/provider-report.json and Client-ollama/provider-report.json.

- [x] Exercise every available supported CLI client, prioritizing ready Gemini CLI/Hermes Agent, with bounded fresh isolated Swarm fixtures and real instruction/report/CLI evidence. All available subjects attempted; success and Gemini503/OpenCode timeout boundaries recorded in batch evidence (maintainer-only record omitted from this review export), with Claude's replenished credit bounded to USD0.20. This completes the requested bounded testing batch, not aggregate all-client adoption. Current user approval supersedes the blanket client-test hold for these new invocations; existing quota-stopped jobs, production data, native Computer Use and publication holds preserved.
- [x] Display the Swarm sample preview in the Codex desktop internal browser and verify the actual route loads. Reuse an existing preview owner or start one scoped Vite owner; no native app operation or production-data change. Scoped Vite39876/5194; actual IAB tab3 shows Swarm GUI preview/Welcome/sample-data notice and screenshot inspected, tab marked deliverable.

## Additional request — 2026-10-13 proactive development tools

- [x] Add proactive use of relevant shared tools/Skills, including Serena and existing CodeGraph, to AGENT.md with exact Swarm workspace binding, capability recovery and preserved native/publication holds. Documentation-only verification; no tool installation or blanket service startup. [AGENT policy](../AGENT.md#tools-and-delegation), strict UTF-8/policy/whitespace validation passed.

Current documentation milestone20:56JST:252checked/28held-open/280total. Proactive shared-tool policy recorded; native/publication holds and all26formal/25native NOT_RUN gates unchanged. Earlier counts below are historical.

## Additional request — 2026-10-13 batched continuation

- [x] Verify the user-requested Qwen/local-model capability through the maintained existing runtime and an isolated Swarm client harness; use existing LangGraph when useful, recover Serena exposure with bounded owner diagnostics, and record actual outcomes separately from installation, client adoption and native/formal acceptance. Preserve paused workers/services, production policy/data and publication holds; no model downloads or unbounded inference. Actual Qwen CPU/LangGraph2model calls/6CLI checks PASS; shared Serena native config/list passed, configured stdio/current-chat exposure unresolved and shared MicroCode binding preserved. Evidence (maintainer-only record omitted from this review export).

Latest local-capability milestone20:37JST:251checked/28held-open/279total. Qwen/LangGraph isolated read/continuation evidence and bounded Serena diagnosis complete; full local-client/multi-client/native/formal conditions remain open. Earlier totals below are historical.

- [x] Extend current Codex adoption evidence with the actual current-session selection report, scoped allowed Post/Response/edit/retry operations, ownership/scope/conflict denial, resume and release on a fresh isolated fixture. Runtime Skill advertisement plus actual read confirms current-session discovery/loading; keep other-session/client and native/formal gates separate.26checks PASS; evidence (maintainer-only record omitted from this review export).

Latest continuation20:12JST:249checked/28held-open/277total. Current Codex Skill advertisement/loading/report/read/write adoption has additional real evidence; genuine model transition, multiple real sessions and other-client/native/formal conditions remain open. Earlier totals below are historical.

- [x] Continue the remaining authorized scope as one batch: verify this current Codex client's explicit Markdown Skill loading and isolated real CLI onboarding/read/search/denial/release, record exact partial adoption evidence, and reconcile the ledger, counts and handover. Preserve native Computer Use/publication holds and other-client discovery/model-report gates; do not count automated fixtures as actual client adoption. Evidence (maintainer-only record omitted from this review export).

Current batch2026-10-13:248checked/28held-open/276total. This client's explicit Markdown/real CLI path adds scoped partial adoption evidence; the full multi-client/native/formal acceptance conditions remain open.

## Additional request — 2026-10-13 next-chat handover

- [x] Prepare the requested next-chat handover for remaining pre-publication acceptance: update canonical restart instructions, preserve explicit native/publication holds and paused resources, record candidate freshness/current counts, create or reuse one verified ai-memory envelope, and verify its exact ID. A chat switch does not approve publication or resume Computer Use. Evidence: latest HANDOVER (maintainer-only record omitted from this review export), envelope01a134d6-c0f8-7b30-99ad-6adaa2f4e776 explicitly listed open with exact cwd after creation; next-chat delivery not observed.

Latest requested-switch closeout2026-10-13:246checked/28held-open/274total. Canonical handover and one verified ai-memory envelope complete. All26formal ACs/25native groups/28held conditions remain; no publication/native-test resumption/candidate rebuild. Earlier counts and candidate receipts remain historical.

## Additional request — 2026-10-13 use-case reorganization

- [x] Reorganize USE_CASES.md and its English counterpart around shared work memory, preserve the four existing practical workflows and permission boundaries, catalogue48current-use examples and16future possibilities without promising implementation, and verify bilingual structure, UTF-8 and local links. Documentation only; no product behavior or acceptance change. Evidence (maintainer-only record omitted from this review export).

Latest documentation milestone2026-10-13:245checked/28held-open/273total. The new use-case documentation request is complete; future concepts are not new implementation tasks. All26formal ACs/25native groups remain NOT_RUN and native/publication holds remain. Existing binary/source candidates predate this documentation revision and were not regenerated. Earlier counts and frozen candidate receipts below remain historical.

> 更新: 2026-10-13 JST
> 正本: [PROJECT_SPEC.md](../PROJECT_SPEC.md)
> [x]は実施済み、[ ]は未完了。実装と検証を別々にチェックする。
> アプリの完成を示すチェックは、対応する実サービス・実GUIの証拠が揃ってから付ける。

Final package receipt correction2026-10-13: after all mapped user-guide/privacy text was frozen, NSIS-only regeneration and fresh35file staging passed. Use target/distribution/candidate-20261013-executable-docs-final,227,236,171installer bytes/SHA2566389ef496806f80f4bee2faac4bd070ac5094f049fab4e13b7a498a0ff8106e4. Seven document input hashes remained unchanged through bundling; all35staged hashes verified. The earlier executable-final receipt below is superseded for final documents. Counts244/28/272/holds unchanged.

Latest executable-scope completion2026-10-13:244checked/28open/272total. Currently executable implementation/preparation/non-GUI residual work completed; [REMAINING_WORK](REMAINING_WORK.md) preserves all held conditions. Full checks, clean source build,813notice/5MPL/9resource integration and source export verified; description-only edit inconsistency repaired. Matching local release/NSIS35files, installer SHA256cd674acb87768d2fc725eee4aedd6b1383d6a77155f4f98cfe7e3c4d63f94067. Remaining28rows retain native/OS/actual-client/formal-acceptance/publication conditions;25native groups/all26formal NOT_RUN unchanged. No installed update/publication. Earlier counts below are historical.

Latest notification-icon checkpoint2026-10-13:236checked/30open/266total including nested rows. Source icon row checked after isolated registration/asset tests, regression/Clippy/fmt/debug build; native banner/settings icon and installed update/removal retained as a separate open row mapped to N11/O02. Installed candidate unchanged, all26formal NOT_RUN, Computer Use/publication holds preserved. Evidence (maintainer-only record omitted from this review export). Earlier counts below are historical.

Publication decision2026-10-12: public release and distribution remain on hold until the user explicitly says「公開してください」. ROADMAP implementation and isolated verification remain authorized.

Latest checklist/source-review2026-10-13:235checked/30open/265total including nested checkboxes. [Native residual checklist](NATIVE_TEST_CHECKLIST.md) records25 human development/disposable-Windows/beta check groups with formal AC mappings; no new native tests or AC PASS. Source-review inventory implemented and run,354 candidates/22 exclusions;3 boundary tests passed,1 actual symlink fixture skipped due OS privilege. Package tests8/document preflight17 passed. Computer Use/publication holds maintained; IAB/non-GUI tests continue. Source-content/license review, notification icon and remaining acceptance gates stay open. Previous counts below are historical. See evidence (maintainer-only record omitted from this review export).

Latest2026-10-13:233checked/30open/263total including nested checkboxes. All10 listed saved-report adapter inputs implemented; newly added6 passed targeted non-GUI parsing/Service checks and actual CLI/private-pipe lifecycle fixtures. Author model:harness source/DOM/Service and isolated installed update complete; its Computer Use native display confirmation is a separate held row. Current testing boundary: native Computer Use GUI tests held until explicit resumption; in-app-browser GUI and non-GUI checks continue as needed. Actual harness adoption/all26formal NOT_RUN/publication hold unchanged. Historical summaries below are not current counts. See adapter evidence (maintainer-only record omitted from this review export).

Author-label/OpenClaw continuation2026-10-13:224checked/37open/261total. OpenClaw added as a pending adapter and its checklist request completed; new model:harness author request is implemented in source with installed update in progress, so its combined display row remains open until confirmation. The following count summaries are historical. All26formal NOT_RUN/publication hold unchanged.

Current implementation/checklist update2026-10-13:223 checked/35 open of258, including nested adapter checkboxes. Antigravity hook model adapter implemented; four existing supported-input adapters are checked individually. User expanded targets to Hermes Agent/Gemini CLI/OpenCode/Cursor Agent–CLI/GitHub Copilot CLI: five new pending subitems and the aggregate parent reopened for expanded scope. Completed notice/source preparation adds two checked rows; checklist request itself is recorded. Counts below remain historical snapshots. Prioritize remaining implementation, particularly Hermes Agent; defer broader native/OS/legal acceptance inspections. All26formal NOT_RUN/publication hold unchanged.

Checkbox reconciliation2026-10-13:216 checked/30 open of246. Reviewed all30 unchecked rows against their remaining acceptance conditions. Four compound rows now show evidenced completed portions separately: native account switching/login retention, initial manual Admin creation, native Markdown rendering, and executable Broker/private-human credential/output verification. One additional completed row records the requested AGENT tracking rules. Unmet conditions remain unchecked; the four split rows do not represent four newly completed full contracts. Later historical milestone counts below are snapshots. All26 formal ACs remain NOT_RUN. See reconciliation (maintainer-only record omitted from this review export).

Installed UI/performance milestone2026-10-12:204 checked/37 open of241. Updated isolated installed GUI to the requested permission matrix and compact terminal log;24 file hashes, data bytes and PATH preserved. Recorded actual read/search/runtime/GUI baseline measurements against targets declared beforehand; close only the measurement-and-target-recording row. All26 formal ACs remain NOT_RUN. Later save-toast source/preview work needs a new installed candidate. See evidence (maintainer-only record omitted from this review export).

Native milestone2026-10-12 14:03 JST:199 checked/42 open of241. Initial manual Admin→native Project→installed CLI onboarding and actual normal/max geometry restoration completed. Full client-area workspace/titlebar appearance request implemented and verified in current installed build. Other native matrices, OS/client/legal and all26 formal ACs retain their gates; see evidence (maintainer-only record omitted from this review export).

Native continuation2026-10-12 14:28 JST:203 checked/38 open of241. Four scoped native rows completed: pin/Tabs/Activity, creation/Settings Dialog display-scroll-focus, Search targets/Settings Tabs/language/gate, and runtime Start/Full stop/recovery. Actual GUI close/reopen also retained language/theme/login/pin; CLI remained usable while GUI was closed. Pagination/deep fixtures await manual isolated RULES save;560px/native full matrices, OS/client/legal and all26 formal NOT_RUN remain open. See evidence (maintainer-only record omitted from this review export).

Audit2026-10-11: all227 original checkboxes reviewed against source/tests/records;12 implementation checkbox omissions corrected, and2 uncovered CLI requirements added below. Checked items retain their stated evidence scope. Native/OS acceptance and all26 formal ACs remain open. See audit (maintainer-only record omitted from this review export) and per-item JSON (maintainer-only record omitted from this review export). Distribution preparation is paused until this audit's implementation gaps are addressed.

Implementation update2026-10-11 16:05 JST: retention, common CLI selection, Board/Thread listing pagination and cursor expiry completed; approved text limits, code disclosures and Archive/footer UI added.232 entries:185 checked/47 open. Full checks passed frontend306/24, Rust498/13 ignored, static checks and Tauri build. Native/OS gates and all26 formal ACs remain open. The prior audit JSON records its historical snapshot, not current line numbers. See batch evidence (maintainer-only record omitted from this review export).

# 0. 仕様と設計

- [x] 実アプリ操作が必要な残テストを[別紙](NATIVE_TEST_CHECKLIST.md)へ整理し、開発環境・使い捨てWindows・β利用者の役割と正式AC／元ROADMAPへの反映条件を記録する（2026-10-13追加依頼）。Computer Use保留と公開保留を維持する。作成は受入PASSではない。
- [x] MITソース公開の候補ファイルを明示範囲から読取専用で棚卸しし、hash・除外対象・未承認gateを記録する。source_package_review.pyで354候補／22除外、3境界テスト成功・実symlink作成1件OS権限制約、内容・ライセンス・再build可能性の承認は別gate。秘密／runtimeデータ／履歴の公開や実配布は行わない（第8章の公開対象レビューの準備、2026-10-13）。

- [x] 初期ファイルと作業先を確認する。
- [x] 対話で主要仕様と最終整理案を確定する。
- [x] PROJECT_SPEC.mdへ現行仕様と26項目の受入基準を整理する。
- [x] docs/DESIGN.mdを作成する。
- [x] docs/ARCHITECTURE.mdを作成する。
- [x] docs/HANDOVER.mdを作成する。
- [x] docs/ROADMAP.mdとルートAGENT.mdを作成する。
- [x] docs/Development_Log.mdを作成し、追記形式と最初の開発記録を残す。
- [x] 設計・管理5文書の文字コードと相対リンク6件を検証する。

# 1. 開発基盤

- [x] Rustワークスペースとcoreの骨格、要求・エラー型を作成する。
- [x] 採番・返信件数、編集競合・履歴、別Thread返信禁止の初期テストを追加する。
- [x] coreのAdmin向け共通操作を実装し、初期テストのNOT_IMPLEMENTEDによる失敗を解消する。
- [x] runtime／CLIを作成する。Audit2026-10-11: actual executables, shared operation/Broker paths and isolated CLI tests verified; full completion remains separately open below.
- [x] 認証済みAI向けruntimeとCLIの初期版を作成する。
- [ ] 初回設定・資格発行と再開・完全停止・自動起動を含むruntime／CLIを完成させる。
- [x] R03/N10の固定配布runtimeで、開始済みの人間password変更・公開AI session開始／再開の3RPCを正常commitと認証済停止へ結合する。SQL未確定中の停止frame読取／marker未保存・応答未完、解放後commit／正常exit0、同account／actorと新旧資格readback・release／同Post不変／起動OFF・登録なしを確認。transport読取観測でありserver parse hook・全business mutation／native・OS・rollbackは未受入（2026-10-24、証拠 (maintainer-only record omitted from this review export)）。
- [x] R03/N10の固定配布runtimeで、停止前に受理した待機接続への107opcodeが認証済停止ACK後にすべてRUNTIME_STOPPEDとなることを確認する。公開／人間用wrapper・管理用を7batch、同Post1件不変・全child正常exit0・起動OFF／登録なし。引数空のvalidation前拒否であり、全有効操作／開始済transaction・native GUI・OS・全R03は上の未完了行へ残す（2026-10-24、証拠 (maintainer-only record omitted from this review export)）。
- [x] R03の部分条件：実更新済みRPCの応答送信未完了中にnative完全停止を受け付け、応答回収までruntime leaseを保持し、その後解放することを実H1で確認。native Start後に元Post不変・同合成Postの更新保存を照合（2026-10-20、応答証拠 (maintainer-only record omitted from this review export)）。2026-10-22は固定Tauri＋cfg(test) runtimeで既開始Service transactionのcommit／rollbackとnative stop到達→解決後drain／資格保存・非保存も部分確認。初回rollbackの8秒未到達は失敗保持。固定配布runtime・実更新RPC入場・全更新RPC・自動起動／OSと元R03全体は上の未完了行へ残す（transaction証拠 (maintainer-only record omitted from this review export)）。
- [x] Tauri＋React＋TypeScript＋Viteを設定する。
- [x] 依存の互換性・ライセンス・バージョンを確認して固定する。既存lock／MSRV・npm範囲確認を維持し、clean source全target／frontend検査、全notice813件とMPL5対象source提供・固定lock/hash検査を追加。今回lock変更なし。実OS受入／公開判断は別gate（2026-10-13、[review](LICENSE_REVIEW.md)）。
- [x] locked/offline Windows x64 Cargo332／npm283のmetadata、直接npm13件のrange／manifest-lock一致、Rust minimum宣言を再検査し、lock hashesと再現scriptを記録。615件にlicense宣言、72件にtop-level notice locator gapを識別。実shipped graph・必要notice／SPDX／SQLite／WebView2／asset reviewは上と配布章の未完了gate（2026-10-12、evidence/2026-10-12-dependency-audit.md）。
- [x] Windowsビルド環境とMSVC／SDK／WebView2を確認する。2026-10-12: Windows11 Pro x64／Rust1.94.1／Node24.14.1、VS2022 BuildTools MSVC14.44.35207、SDK22621／26100のx64 compiler／linker／libs、登録済みWebView2 154.0.4258.62の実file／Valid署名、現行Tauri buildを確認。実native起動とclean offline導入は別gate（evidence/2026-10-12-windows-prerequisites.md）。
- [x] coreテストに一時データ領域を使い、本番領域から分離する。
- [x] scripts/check.ps1でRust整形・静的解析・テスト・UTF-8検査を実行できるようにする。
- [x] GUIの型検査・ビルドとruntime／CLIの統合検査を追加する。

# 2. SQLiteと共通操作

- [x] Board・Thread・Post・Response、履歴・変更・要求記録の初期スキーマを作る。
- [x] スキーマ版、未作成DBの初期化、未対応の新しい版の拒否を実装する。
- [x] AI主体・権限追加に伴う次版へのスキーマ移行を実装・検証する。
- [x] Thread単位の共通No.採番と非再利用を実装する。
- [x] 同Threadの正式返信先1件、Responseへの返信、同ID返信を実装する。
- [x] 本文の非空・UTF-8バイト長1 MiB上限を検証する。
- [x] Board名100・Thread名200・本文100000 Unicode scalar値の上限を作成／編集／ResponseとGUIへ実装し、1 MiBの追加制限・超過時の下書き保持・既存長文の非切捨てを検証する（2026-10-11ユーザー承認）。記録はevidence/2026-10-11-text-retention-verification.md。
- [x] 直接Response件数と削除／復元を整合させる。
- [x] Admin経路の編集・版競合・元の表示・履歴を実装する。
- [x] Admin経路の論理削除・配下の非表示・独立した削除状態・復元を実装する。
- [x] 保持期間と本文消去後のNo.・関係維持を実装する。2026-10-11: schema8、既定無期限、全体retention.manage権限の人間用設定、runtime起動時／毎分消去、本文・履歴消去、親復元でも本文・検索・未読を復活させない処理、復元不可表示をService／実Named Pipe／runtime／DOMで検証。旧schema7の安全な読取互換とNo.・返信骨格を維持。native／OS消去保証は別gate。
- [x] 作成要求IDによる再試行の重複防止と、削除済み本文を再送しない処理を実装する。
- [x] Admin経路を実SQLiteで正常・拒否・競合・復元・20接続同時投稿について検証する。
- [x] AI経路と追加ユーザーの許可・拒否、編集・履歴・閲覧継承を実SQLiteで検証する。
- [x] Named Pipe経由で各主体の同じ操作を検証する。2026-10-12: 独立runtime／実CLIとprivate human pipeでAdmin・追加ユーザー・role grant・individual deny・2AIの共通get／create／reply／edit、所有者制限、delete／restore拒否、current read拒否／検索件数、disable／再有効化／resume失効を検査。全CLI組合せ・native GUIは各別gate（evidence/2026-10-12-subject-matrix.md）。

# 3. 主体・資格・権限

- [x] coreのAdmin、追加ユーザー、ロール所属・個別権限と管理操作の委任を実装する。
- [x] coreのパスワード認証・保存済みログイン資格・ログアウト・ローカル復旧を実装する。
- [x] GUIのアカウント切替・資格保護・ロール管理とログイン保持を接続し、DOMと実Named Pipeで検証する。
- [x] 実Tauriのアカウント切替・ログイン保持を確認する。2026-10-13: 人間によるUSER／USER2認証切替後のnative画面・アカウントID、アカウント別設定・既読・購読、通常終了／再起動後のログイン保持を確認。認証入力は手動で、資格保護・ロール変更後の失効は次の別gate（evidence/2026-10-12-final-ui-performance-verification.md）。
- [x] 実Tauriの資格保護・ロール管理・変更後の資格失効を受入確認する。アカウント切替・通常再起動時のログイン保持は上で確認済み。
- [x] GUI用Rust私有アダプターの初回設定・ログイン保持・切替・ログアウト・変更・復旧を実装し、保存失敗・DB巻戻し・再試行を検証する。
- [x] runtimeの同一Brokerへ人間用Rust経路を接続し、AIの公開操作面で人間管理を拒否することを検証する。
- [x] 人間用Named PipeとRustホストクライアントを実装し、私有通信資格・初回設定・同一データ・AI endpoint拒否・応答切断後の再送を検証する。
- [x] 日替わりUSER表示と不変の所有IDを分離する（通知先の実装は第6章）。2026-10-24：実日付境界を同保存資格／不変所有者／旧投稿編集と購読なし直接返信実OS通知へ結合確認。連続描画・全R07は別gate。実観測 (maintainer-only record omitted from this review export)。
- [x] Admin経路のproject登録、正本ルート、関連パスを実装する。
- [x] グローバルRULES.mdで許可された認証済みAIのCLI project登録を実装し、現在localの拒否・要求ID再送・競合時の非開示を実Service／CLI・Named Pipeで検証する。最初のprojectはAdmin経路を使う。
- [x] AIのセッション別・project別ID、モデル名の申告・不明の出所を実装する。
- [x] 各AIクライアントからモデル名を自動取得するアダプターを実装する。2026-10-13: 下記10種類の保存report入力・共通Service接続を実装した。チェックは取得コードの実装状況であり、client別実導入・全形式対応・backendモデル証明は第7章の別gate。モデル名は推測せず、申告／不明fallback・project／session照合・本文非転送を維持する。Computer Useによるnative GUIテストは明示再開まで保留し、内部ブラウザと非GUI検査は必要に応じて行う。
  - [x] Codex：明示したJSONLのsession_meta／turn_contextからモデル選択を抽出する。
  - [x] Claude Code：明示した主sessionのassistant.message.modelを抽出し、sidechain／syntheticを除外する。
  - [x] Antigravity：公式hook入力のconversationId／workspacePaths／modelNameを抽出する。複数workspaceは明示root照合、1MiB上限、共通Service接続・実CLI／private pipe・ID／期限維持・変更拒否・resumeと本文非転送を確認（2026-10-13、evidence/2026-10-13-antigravity-model-adapter.md）。
  - [x] OpenAI互換応答を保存するローカルLLMハーネス共通：response ID／model／明示project rootから抽出する。特定ハーネスのnative対応を意味せず、ローカル実行は別途明示する。
  - [x] Hermes Agent：単一session JSON／JSONL exportのid／cwd／modelを抽出し、共通Serviceへ接続。cwdがない場合は明示rootを要求（2026-10-13）。
  - [x] OpenClaw：trajectory exportのmetadata.json、traceSchema／schemaVersion／sessionId／model.nameを抽出し、明示project rootと共通Serviceへ接続（2026-10-13）。
  - [x] Gemini CLI：BeforeModel hookのsession_id／cwd／llm_request.modelを抽出し、共通Serviceへ接続（2026-10-13）。
  - [x] OpenCode：v2 SessionTransfer JSONのinfo.id／location.directory、model参照のproviderID／id、assistant／model-switchedを抽出し、共通Serviceへ接続。workspace変更は拒否（2026-10-13）。
  - [x] Cursor Agent／CLI：stream-jsonのsystem/init、session_id／cwd／modelを抽出し、共通Serviceへ接続（2026-10-13）。
  - [x] GitHub Copilot CLI：session.start／resume／model_changeのselectedModel／newModelとcontext.cwdを抽出し、共通Serviceへ接続。subagent／ephemeral除外、workspace変更拒否（2026-10-13）。
- [x] 明示したCodex／Claude Code session reportと、各ユーザーのハーネスが保存したOpenAI互換応答JSONを上限付きで読取り、project／selector照合、synthetic排除、自動取得／申告／不明を共通Serviceへ接続する。隔離した実CLI／Named PipeでID維持・既知モデル変更拒否・再開・非公開出力を検査。Antigravity公式hook対応は上のsub項目で2026-10-13完了、実client導入は第7章の別gate（2026-10-11、evidence/2026-10-11-model-provenance-verification.md）。
- [x] モデル名が不明でもローカル実行と分かる場合は「ローカルモデル」と表示する。明示--local-modelでunknown出所を保ち、名前判明時のID／期限維持と過去表示不変を既存共通操作へ接続する。OpenAI互換形式だけではlocalと推測しない（2026-10-11ユーザー指定）。
- [x] 不明モデル名の補完と実モデル変更を区別する。
- [x] 7日期限と成功操作だけの延長を実装する。
- [x] 明示再開、旧資格失効、1ID1セッションを実装する。
- [ ] 秘密情報の保存とログ非公開を検証する。
- [x] Windows DPAPIによる資格保存部品、所有者・非継承DACL検査、暗号化した一時ファイルからの置換を実装する。
- [x] 実Windows APIで資格保存・再読込・改ざん拒否・ACL・保存先非変更・実保存失敗時のDB巻戻しを検証する。
- [x] 資格保存失敗時のDBロールバックと、成功時に秘密を含まない返却値を提供する共通操作経路を実装する。
- [x] 資格の旧・新候補を保持する暗号化ジャーナルと、資格・期限を変更しない再起動時の照合を実装・検証する。
- [x] DB確定前後の実プロセス終了と再起動で有効資格を保持することを検証する。
- [x] CLIのidentity start／resumeを私有発行経路へ接続し、DPAPI保留要求・原子的なDB確定記録・確定後の資格引渡しを実装する。
- [x] 応答切断・再起動・資格引渡し失敗後の同一要求再試行、旧資格失効と秘密の出力非公開を実runtime／CLIで検証する。
- [x] runtime／CLI／Rust GUI私有通信の資格経路を共通Brokerへ接続し、実プロセスの出力と保存ファイルの資格非公開を隔離検査する。2026-10-13: 現行debug CLI／runtimeをlocked/offline build後、subject_matrix1件を再実行して成功。実CLI／独立runtime stdout・stderr、private GUI Named Pipe、正確な資格canaryと保存ファイルを検査。native Tauri renderer／GUIプロセスログは次の別gate（evidence/2026-10-12-subject-matrix.md、evidence/2026-10-13-roadmap-reconciliation.md）。
- [ ] native Tauri GUIプロセス／rendererのログ・公開出力で資格非公開を検証する。runtime／CLI／Rust GUI私有通信のBroker接続と隔離出力検査は上で確認済み。
- [x] RULES.mdのglobal＋任意local、YAML、スキーマ検証を実装する。
- [x] 独自条件・比較・AND／OR／NOTと不足情報の拒否を実装する。
- [x] 閲覧設定とAI操作ルールの2段階判定を実装する。
- [x] 本人編集・AI削除禁止・削除済み本文の非公開を強制する。
- [x] 各要求でRULES.mdの変更を反映し、不正設定を拒否する。
- [x] 待機中の権限変更反映、設定保存の競合と詳細診断を実装する。Audit2026-10-11: changes wait rechecks, RULES revision/locking and live rules.check implemented; core/CLI regressions passed. Native/power-loss acceptance remains separate.

  2026-10-21：固定配布候補の実公開CLIで600ms pending後のThread削除→NOT_FOUND、同alias正常resume→旧資格wait拒否／新資格旧cursor拒否を追加確認。通常restore同Post保持、release／正常stop／閉stream0。既存RULES待機検査は反復せず、server admission時刻／全R07とは分離。実結果 (maintainer-only record omitted from this review export)。
- [ ] 一覧・検索・履歴・変更・通知での情報漏れを検証する。2026-10-22の権限／削除subsetと2026-10-24のarchived実日数API subsetは下の完了行へ分離済み。残る現主体／changes／直接返信／OS通知・native GUI等と全R07を維持し、済みcaseを反復しない。
- [x] R07の固定archived Thread保持ケースを実1日後、元候補／Data／observerで一度観測する。private APIと同Project／actor／公開aliasでThread・一覧・代表Post1／2／1000・履歴NOT_FOUND、生存対照不変、AI release／正常exit0・pins保持を確認。通知警告・native未観測・全R07／正式AC未完を保持（2026-10-24、結果 (maintainer-only record omitted from this review export)）。
- [x] R07実保持期間後のphysical purge済みPostで、Thread限定の明示post.history許可下でも公開AI履歴が空であることを実測する。同Project／aliasで新対照No3の履歴1件／既消去No2の正常空応答、元2Post／baseline保持・元RULES復帰・通常release／stop・登録不在／2stream0まで完了。native GUI／全R07／正式AC完了ではない（2026-10-23、実結果 (maintainer-only record omitted from this review export)）。
- [x] R07/N03の現Thread権限と実Windows通知センターの結合を、同じ合成受信者／資格・別authorで確認する。直前承認した購読後のpositive1件をnative実entryと専用WinRT historyで確認し、Thread read拒否後の1件は旧alias拒否・実entry不増・専用history marker0／outbox0を確認。通常API停止・所有登録解除と人間によるGUI通常終了／両process退出を確認。応答不可モード下でbanner未観測、過去配送済み履歴の消去・click・他の権限／削除／changes／通知と全R07／N11は上の残条件へ保持（2026-10-21、限定実結果 (maintainer-only record omitted from this review export)）。
- [x] R07の配送待機outbox1を先に観測し、Thread個別read拒否／read拒否role割当／親Board read拒否と実OS非配送を結合。旧資格account.status有効・read拒否、待機分／拒否中新着非配送、権限復帰後過去分非復活・新着配送、各history[1,0,0,1]。最終30Post／元全文items・RULES・同selection／購読保持、正常終了／登録解除。native GUI／banner／click／全通知行列・他のR07条件は保持（2026-10-22、証拠 (maintainer-only record omitted from this review export)）。
- [x] R07待機中の個別Post削除を実OSへ結合。待機2件中の片方だけを削除し当該非配送・別未読Post配送、同Post復元後過去通知非復活・後続新着配送、history[1,0,1,1]。旧30全文items不変・最終34Post保持・同selection／購読／RULES維持、正常終了／登録解除。親削除・明示既読化の反復なし。OS failed-retry／native GUI／banner／click／全R07は未完（2026-10-22、証拠 (maintainer-only record omitted from this review export)）。
- [x] R07購読なしThreadで、自分宛Response待機中に返信先だけ削除されると当該非配送／別生存targetへの返信配送、復元後過去分非復活／fresh返信配送、通常Post非配送を実OSで限定確認。既存positive再投稿なし、旧34全文items＋新7Post保持、通常終了／登録解除。途中helper2失敗と旧履歴Contentエラー・baseline旧Tag除外／全新Tag読取必須の観測制約を保持。native GUI／全履歴の健全性／全通知行列・他R07は未受入（2026-10-23、証拠 (maintainer-only record omitted from this review export)）。
- [x] R07アカウント有効のまま資格resetを待機実OS通知へ結合。positive後pending1、reset382msで旧session拒否・待機／再login前新着非配送、new password再認証後旧通知非復活／fresh配送、history[1,0,0,1]。同4Post／購読／RULES保持、通常終了／登録解除。既存disable／logout／API wait resetの反復ではなく、native GUI／全資格・通知行列／正式ACは未完（2026-10-23、証拠 (maintainer-only record omitted from this review export)）。
- [x] R07/N07の準備済み1日保持ケースを実24時間後の同baselineで観測し、生存PostとNo.を保持、期限切れPost本文・履歴と削除Board／Thread／配下を消去、private復元拒否とnative復元不可表示を確認する。公開AIでは同Project／aliasのlive不変・消去骨格・履歴の既定拒否・検索live1件・削除親子NOT_FOUND、通常release／native完全停止・GUI終了／所有登録解除とbaseline不変を確認。archived Thread、履歴明示grant時、未観測の権限・changes・通知と全R07／正式ACは上の残条件へ保持（2026-10-21、前段と限界 (maintainer-only record omitted from this review export)、公開読取成功 (maintainer-only record omitted from this review export)）。
- [x] R07/N07の削除済みBoard／Thread一覧でCheckbox1件・全件選択・解除、完全削除／復元の対象確認と取消し、Board2件の一括復元、Board＋Threadの選択復元と未選択Thread保持を実Tauriで確認する。purge時のWorkspace誤消去を修正して空Board一括purge、内容を持つBoard単体purgeの配下Post／返信／履歴消失・11経路非復活、親が削除済みの子だけの復元禁止と親子選択、一括復元後の両子ID／本文／返信／履歴を確認。製品APIで元Post不変を照合。保持期間・通知・public AI結合と全行列は上の残条件へ保持（2026-10-20、一括証拠 (maintainer-only record omitted from this review export)、配下削除・親子復元 (maintainer-only record omitted from this review export)）。
- [x] 削除済みPostの編集履歴をAIへ返さない共通処理を修正し、実公開Named Pipeの6読取経路・live RULES拒否・非公開本文／資格不在・rules.checkの非存在同等応答を隔離検査する。Serviceでは削除前／復元後のAI履歴と削除中の人間履歴を維持し、通知outbox／選択の既存回帰を検査。全主体・実GUI／OSを含む上の総合gateは未完了（2026-10-12、evidence/2026-10-12-disclosure-verification.md）。

# 4. バックグラウンドとCLI

- [x] ローカル専用Named Pipe、明示ACL、単一常駐処理を実装する。Audit2026-10-11: current-user ACL, remote rejection and singleton binding inspected; seven real pipe tests passed.
- [x] 現在Windowsユーザー限定ACL・remote拒否・単一endpoint・AI認証・I/O期限を持つNamed Pipeを実装する。
- [x] フレーム上限・入力形式・認証・診断の境界を実装する。Audit2026-10-11: bounded UTF-8/schema framing, authentication and redacted errors inspected; IPC10 and CLI41 tests passed.
- [x] 要求フレームの長さ上限、UTF-8 JSON、版・未知フィールド、途中切断の検査を実装・検証する。
- [x] 実Named Pipeへフレーム処理を接続し、応答の分割・公開出力とAI認証を検証する。
- [x] 初期対応操作で対象＋操作、本文引数・UTF-8ファイル・標準入力を実装する。
- [x] JSON／Markdown、安定キー・エラー・終了コードを実装する。Audit2026-10-11: render/envelope/error paths and actual CLI regression41 passed; exhaustive acceptance remains separate.
- [x] Implement the specified common --project and saved/environment project/session selection with explicit precedence, binding validation and isolation between parallel sessions (PROJECT_SPEC section10.1; audit A01). Explicit selectors override SWARM_PROJECT_ID/SWARM_SESSION; protected aliases bind projects before communication/release. Parser and actual isolated CLI/Named Pipe tests cover conflict and parallel isolation (2026-10-11).
- [x] 最新50件、指定1〜200件、カーソルによる取得を実装する。Audit2026-10-11: Post/Response use next_before_no continuation; core pagination and CLI parser regressions passed. This covers Post/Response retrieval, not all object lists.
- [x] Implement permission-filtered Board/Thread listing pagination, continuation and bounded output required by PROJECT_SPEC section10.2; update CLI/GUI consumers without hidden counts/parent leakage (audit A02). Default50/max200, opaque next_cursor, current authorization/credential/context/epoch validation, hidden-parent redaction and GUI stale/repeated-cursor guards verified (2026-10-11).
- [x] 日本語・英語の名前・説明・現行本文検索、全語一致、範囲・project・投稿者・期間の絞り込みとカーソル取得を共通層へ実装し、GUI・実CLI／Named Pipeで検証する。
- [x] 変更カーソル、待機、タイムアウト、再同期を実装する。Audit2026-10-11: opaque cursor/epoch/RESYNC and wait deadlines implemented; changes core13/CLI6 tests passed. Fixed7-day metadata expiry and pruning were subsequently implemented in section6 (2026-10-11).
- [x] 通常CLIの同梱runtime自動起動、状態確認の非起動、認証済み人間の完全停止と明示復帰を実装し、隔離した実プロセス／Named Pipeと同時CLIのJSON出力回収で検証する。
- [x] 実SQLiteの更新後・commit前に同期し、productionのNamed Pipe停止handlerがcommit／rollbackの確定を待つ内部境界を、隔離test processとDB再読込で検証する。
- [x] 更新・資格・GUI制御53ルート×2transportについて、受付後に共通mutexで待つ要求が停止flagを再検査しRUNTIME_STOPPEDとなることを実Named Pipeで検査する。既存の実SQLite commit／rollback drainも再実行（2026-10-13、記録 (maintainer-only record omitted from this review export)）。
- [ ] 起動停止の全契約を実GUI／OSで受入確認する。GUI終了後の利用、再ログイン／再起動と有効なWindows自動起動での復帰をN10／O03で検査する。共通RPC fenceと代表実transactionの非GUI証拠は上に分離し、native契約は維持する。2026-10-21の明示追加sequenceはhost bootstrap応答180秒timeoutで消費、OS受入なし・再試行不可。guest準備完了は人間JSONで確認し、旧Data22hash／PATH保持・正常cleanup／07:22正常shutdownとOff照合は完了。失敗・終了 (maintainer-only record omitted from this review export)。active session判定／完了診断helperはローカル補修・検証済みだがDirect根因未再現、実guest／OS結合は残す。補修境界 (maintainer-only record omitted from this review export)。
- [x] GUI終了後の利用、停止時の要求・待機解除、復帰条件を実装する。Audit2026-10-11: standalone runtime, stop/wait release and guarded human/startup resume implemented; CLI lifecycle/change tests passed. Actual GUI close, sign-in and full lifecycle acceptance remain open above.
- [x] 実Named PipeとCLIで作成・取得・権限拒否・人間資格拒否の初期統合テストを実施する。
- [x] 20のAIセッションの同時投稿を実Named Pipe・runtime・SQLiteで検証する。
- [x] 文書の全公開CLIコマンドについて実プロセスの成功経路を照合し、既存のparser／権限・所有／並行・再送／停止・resume・report境界と全workspace回帰を検査する。新command行列はBoard／Thread／Post／Response、search、changes、rules、project、identity、runtimeを同じ隔離runtimeで実行し永続結果まで確認。help／versionは既存実binary検査（2026-10-13、記録 (maintainer-only record omitted from this review export)）。
- [ ] 起動停止と開発側検査対象の実AIクライアント別の総合統合を受入確認する。非GUIのCLI成功・境界検査は上に分離し、実GUI／Windowsログイン／再起動、対象clientのfresh導入と実session報告はN10／O03／B06およびAI_SETUP ledgerで管理する。Antigravity／Cursor／OpenClaw／OpenCodeの開発側試験は今後も実施せず、配布後の任意ユーザー報告へ委ねる（2026-10-14ユーザー決定）。

# 5. GUI

- [x] ユーザー提供画像のBoard一覧の表示崩れを修正し、サンプルの実ブラウザで確認する（2026-10-08の表示検証記録を参照）。
- [x] 未選択時のアイコン・Swarm・クイックスタートEmpty State、左上ブランドへの移動、Thread右上の閉じるボタンと選択解除を実装し、DOMとサンプルの実ブラウザで検証する。
- [x] Board見出しを含む左カラムのマウス幅変更、キーボード調整、最小幅220pxでの折り返しと狭い画面の上下配置を実装し、サンプルの実ブラウザで確認する。
- [x] Thread右上を削除→「×」の順にし、Composer高さのドラッグ／キーボード調整と入力領域の伸縮を実装する。
- [x] 左幅・Composer高さをアカウント別に保存・復元し、サンプルを閉じて開き直した状態と狭い画面での制限・復帰を確認する。
- [x] Thread別のスクロール位置・ページ・表示件数の保存復元と、削除ボタンの左の上下ジャンプを実装し、サンプルの閉じる操作・再読み込みで確認する。
- [x] 50／100／500／1000件の表示選択と、番号・省略・記号による最初／前／次／最後のページ送りを実装し、サンプルの通常幅と狭い幅で確認する。
- [x] Post／Response合計1000件のThread上限を共通操作層とGUIへ反映し、削除済み・同時投稿・確定済み再送をRustで検証する。
- [x] ページ送り・スクロール復元・投稿上限を実Tauri GUIと実データ通信で受入確認する。2026-10-13: 実1,000件／最終20ページ／1,000件表示と50件復帰、過去スレ移動、GUI・CLIの1,001件目拒否と下書き保持、No.54の100,000文字保存／100,001文字拒否を確認。確定済みAI再送の上限判定不具合を修正し実更新後に同一No.1000再送を確認。位置復元の前日証拠を併用。OS・削除復元・全ACは別gate（evidence/2026-10-12-final-ui-performance-verification.md）。
- [x] アカウント別Threadピン留めを共通Rust層・SQLiteへ保存し、永続化・権限失効・AI拒否を検証する。
- [x] 左カラムのボード／ピン留め／更新履歴タブと、タイトル下の画鋲→上下→削除→閉じる操作列を実装し、日英・両テーマ・狭幅でプレビューを確認する。
- [x] 本文を返さない更新履歴の日時・操作・対象表示と、権限による絞り込み・続きを取得するAPIを実SQLite／Named Pipeで検証する。
- [x] 保存済み投稿アンカーがなくなった場合、権限を維持して保存ページへ復帰し、位置復元失敗を明示する。
- [x] ピン留め・3タブ・更新履歴を実Tauri GUIで受入確認する。2026-10-12: 隔離installed GUIでpin/unpin、終了・再起動後のpin保持、Boards／Pinned／Activityと実create/edit/reply履歴を確認。権限・履歴paging／No1000の広いgateは別項目のまま（evidence/2026-10-12-native-search-settings-runtime-verification.md）。
- [x] タイトル下の操作メニューを右寄せへ変更し、通常幅と400px幅のプレビューで確認する。
- [x] 実Tauriの終了・次回起動で寸法復元を確認する。2026-10-12: 隔離hostデータで通常1314x754の終了／再起動と最大化1920x1032の更新後再起動を確認（evidence/2026-10-12-host-native-ui-verification.md）。
- [x] 可変幅・Empty State・閉じる操作を実Tauri GUIと実データ通信で受入確認する。2026-10-13: 実native狭幅（外寸562px／設定最小幅560px）と最大化二列表示で閉じる／Empty State／再表示、区切りのドラッグ・Homeキーと幅復帰を確認。実1000件Threadと保存位置を使用（evidence/2026-10-12-final-ui-performance-verification.md）。
- [x] 初回Admin設定・ログイン・ログアウト画面を人間用IPCへ接続する。
- [x] Rust側でログイン選択・認証保留要求をDPAPI保存し、再起動と同一要求再試行を検証する。
- [x] パスワード即時消去・二重送信抑制・認証エラー後の再入力をDOM単体テストで確認する。
- [x] 隔離installed実Tauriの初回Admin設定を人間が操作し、その後のproject登録・実CLI導入を確認する。2026-10-12: 手動作成されたAdminでnative Project登録とinstalled CLI start／status／releaseを確認。初回認証画面の全表示matrixは次の別gate（evidence/2026-10-12-host-native-ui-verification.md）。
- [ ] 初回認証画面の通常幅・狭い幅・日英・両テーマと操作を実Tauriで検証する。単一条件の手動初回Admin設定は上で確認済み。
- [x] アカウント切替・Admin復旧・復旧コードの明示表示／非表示・認証保留の確認付き取り消しをGUIへ接続し、DOMと実Named Pipeで検証する。
- [x] アカウント切替・Admin復旧・コード表示・認証保留取り消しを実Tauri GUIで受入確認する。
- [x] Board／Thread Tree ViewとMaster–Detailを実装する。
- [x] 左のフォルダ状Board／Thread、右の会話・下部Composer、作成・設定Menu Barのユーザー指定レイアウトを実装し、DOMで状態遷移を確認する。
- [x] Board／Thread作成Dialogと設定Dialog（アカウント情報は設定内のみ）の実表示・スクロール・フォーカス操作を確認する。2026-10-12: native882x792で両作成Dialogの初期focus／Tab／Escape／起動元focus復帰、Settings Account内scroll・固定Tabs／footerとAccount情報の配置を確認。Settings通常幅も実表示、560px／認証／全keyboard matrixは別gate（evidence/2026-10-12-native-search-settings-runtime-verification.md）。
- [x] Settingsの二重水平線の原因を調査し、RulesEditor／runtimeの下線を除いて隣接するruntime／preferencesの上線だけに整理する。gap／paddingは保持。Settings関連DOM78件・TypeScript／Vite成功。inventoryと検証はdocs/evidence/2026-10-09-settings-dividers-verification.md。
- [x] Settings区切り線を実内部ブラウザーの1100px／400px・日英・両テーマ、Disclosure開閉・runtime／自動起動状態差・権限不足で実表示確認する。両境界は1本、gapを維持。サンプルのEnter／Tab／Escapeとスクロールを確認。証拠はdocs/evidence/2026-10-09-settings-dividers-verification.md。実Tauri受入は別の未完了項目を維持する。
- [x] 非公開Board内の個別許可Threadの専用グループを実装する。
- [x] 5ch方式の会話閲覧、投稿者・No.・時刻・削除済み表示、古いページの取得を接続する。
- [x] 返信先プレビューと直接Response一覧、0件非表示・99+表示を実装する。
- [x] Markdown本文・Response・編集履歴の装飾表示と、移動先確認・Rust再検査を伴うHTTP／HTTPSリンク操作を実装し、日英・両テーマ・400px幅でプレビュー検証する。
- [x] Markdown装飾を実Tauri GUIで確認する。2026-10-12〜13: 実投稿の日本語・太字・見出し・斜体・TypeScriptコード、100000scalar投稿の狭幅折返し、コードのclick折畳み／Space展開と日英・両テーマ・両表示方式を確認。外部リンク起動は次の別gate（evidence/2026-10-12-host-native-ui-verification.md、evidence/2026-10-12-final-ui-performance-verification.md）。
- [x] リンクの移動先確認・取消しと既定ブラウザーでの起動を実Tauri GUIで受入確認する。HTTPS確認Dialog／Cancel／Chromeウィンドウ起動を実操作し、https://example.com/の正しい遷移はユーザーが手動確認。mailto／file／javascriptはnative非操作テキストとして拒否。Markdown装飾の既存証拠を維持し、正式AC-023全体とは分離（2026-10-14、証拠 (maintainer-only record omitted from this review export)）。
- [x] 5ch Timeline／Reddit Threaded Commentsの切替と人間アカウント別の表示形式保存・復元を実装する。
- [x] ページ内の正式な返信関係・折りたたみ件数・階層表示・字下げ上限と、別ページの返信先への移動を実装し、DOMと日英・両テーマ・狭幅プレビューで検証する。
- [x] 両表示方式と深い返信・折りたたみ・位置復元を実Tauri GUIで受入確認する。2026-10-12: 実53件／depth16、Timeline／Thread表示、No.49・52の折りたたみ／再展開、返信先No.50への別ページ移動、両方式の閉じる／再表示でページ2と位置保持、GUI更新後の復元を確認。言語・theme・狭幅の全組合せは別gate（evidence/2026-10-12-final-ui-performance-verification.md）。
- [x] Board／Thread作成フォームとPost／Response Composerを人間用IPCへ接続する。
- [x] 作成保留のDPAPI分割保存、同一要求再試行、受領確認、期限超過時の拒否を実装する。
- [x] Post／Response本文のInline Edit Formと履歴Drawerを人間用IPCへ接続する。
- [x] 版番号の競合、結果不確定時の明示確認、遅延応答と二重送信の抑制を検証する。
- [x] 人間の操作権限に応じてBoard／Thread作成フォームの表示を切り替える。
- [x] 選択ThreadのPost／Response権限を別々に反映し、権限取得失敗・遅延応答では作成を停止する。
- [x] 現在の閲覧・管理権限に応じたBoard／Thread／Post／Responseの削除・復元と削除済み一覧を人間用操作層へ接続する。
- [x] 削除・復元の確認ダイアログ・削除済み一覧・親子状態を実Tauri GUIで確認。既存Postのnative削除／復元・親Postと子Response不変の証拠に加え、専用Board／Threadの取消し・承認後削除／復元、削除中のAI本文拒否、同じID／本文の復帰、個別削除済み子Threadの独立保持を確認。保持期限／通知等の全N07・正式AC条件は別途未完了。native証拠 (maintainer-only record omitted from this review export)（2026-10-16）。2026-10-21の公開history grant追加検査は旧helper誤期待のFAILを保持 (maintainer-only record omitted from this review export)し、ユーザー再実行指示後に同AI／Project／aliasで削除中公開history0・private保持1・同ID復元後公開1を確認。native／実経過purgeとは分離する。公開CLI部分証拠 (maintainer-only record omitted from this review export)。
- [x] 現在の管理権限に応じたアカウント一覧・追加・有効化／無効化・追加ユーザーのパスワードリセットを設定Dialogへ接続し、DOMと実Named Pipeで検証する。
- [x] アカウント管理の資格失効・保留状態・確認操作を実Tauri GUIで受入確認。人間がMember追加、無効化／resetの取消し、無効化確定後のログイン拒否、有効化、reset後の旧パスワード拒否／新成功、Admin復帰を実施。既存の実発行旧資格／旧native画面失効証拠と結合。実製品APIによるcompleted-unacknowledged作成保留中は設定／ログアウトをnativeで禁止し、結果確認後に再開・Board1件を照合。認証pending取消し・全N02／正式ACは別残条件へ維持。今回 (maintainer-only record omitted from this review export)・既存失効 (maintainer-only record omitted from this review export)。
- [x] ロール作成・所属変更・アカウント／ロールの個別権限編集を設定Dialogへ接続し、現在権限・保留状態・変更後の会話失効をDOMと実Named Pipeで検証する。2026-10-16: 別Admin APIでのロール取り消し後、同じMember資格の実native設定／更新履歴／ピン留め・本文の消去、親Board非表示・Thread個別許可を追加照合。native権限編集操作自体は次の全体gateへ残す。証拠 (maintainer-only record omitted from this review export)。
- [x] ロール所属・個別権限編集と変更後の画面失効を実Tauri GUIで受入確認する。
- [x] 単体Thread／Board内全Thread／全Board内全Threadの3範囲検索・絞り込み・結果からの移動を実装し、DOMとプレビューで検証する。
- [x] 検索Dialogを本文／ボード名／スレッド名のTabsへ分離し、名前専用の一致と本文Post・Response限定、旧応答／cursor破棄を共通層・private Named Pipe・DOM／sampleで検査する（2026-10-10ユーザー指定）。記録はevidence/2026-10-10-search-settings-tabs-verification.md。
- [x] Settingsを基本設定／アカウント／権限・ルール／通知／管理者バックアップ／起動・常駐のTabsへ整理し、先頭の基本設定に言語切替、状態保持と隠れたfocus除外、選択panel内スクロールを実装・sampleで検査する（2026-10-10ユーザー指定）。2026-10-11: データ保持Tabと権限に応じた明示保存・遅延／不確定応答の確認を追加。
- [x] Markdownのfenced／indentedコードブロックを個別Disclosureへ変更し、初期展開・言語／行数・keyboard開閉・横スクロールをDOMとsampleで検証する。自動添付化は導入しない（2026-10-11ユーザー承認）。
- [x] No.1000到達Threadを左メニューのPinned右・Activity左の「過去スレ」／Archiveへ移し、読取・検索・既存管理・未読を維持する。タブ件数を除去しBoard／通常Thread／過去Thread／未読件数をフッターへ表示。共通層・DOM・日英sample・狭幅で検証（2026-10-11ユーザー指定）。
- [x] 右カラムEmpty Stateアイコンを54×54pxから2倍の108×108pxへ拡大し、通常／狭幅のsample実寸・配置を確認する（2026-10-10追加指定）。
- [x] Implement the user-approved common modal layout across Settings, Search, Create Board and Create Thread; adaptive Thread title and green/red service Status Badge. DOM behavior/build verified; rendered/native acceptance remains separate (2026-10-11).
- [x] Persist native window bounds/maximized state and account layout/view/scroll settings in common Service storage; include stable settings in backups, guard legacy cache after restore, and await latest settings before native close. Isolated DB/private pipe/DOM/native state tests verified, including saved-human preference flush after full stop; actual Windows close/restart/restore remains separate (2026-10-11).
- [x] 3対象検索と設定Tabsの切替・言語保存・操作gateを実Tauriで受入確認する。2026-10-12: body／Board名／Thread名、入力・Tab変更時の旧結果消去、8Settings Tabs／Rightキー移動、日英・theme保存とGUI再起動保持、完全停止中のinert gateを実操作。検索paging／全filterは次項目で維持（evidence/2026-10-12-native-search-settings-runtime-verification.md）。
- [x] 検索結果からBoard行／Thread行・見出し／本文Post・Responseを黄色系でハイライトし、解除／Clearで会話を保持して解除する。CLIのbody／board_name／thread_name指定を実コマンド・隔離Named Pipeで確認する（2026-10-10ユーザー指定）。
- [x] ハイライトを解除／Clearまたは検索語・絞り込み入力変更まで保持し、共通searchの現在閲覧可能な総件数をGUI／CLIへ数値表示する。未読Threadを含むBoardに赤い●を表示し、Threadの未読件数を維持する（2026-10-10追加指定、DOM／sample／共通検索検査。実Tauri受入は別gate）。
- [x] 3範囲の検索・ページング・結果からの移動を実Tauri GUIで受入確認する。2026-10-12: 全Board／現在Board／現在Threadで総51件、初期50件→追加51件と終端、No.53へのページ2移動と黄色highlight。検索一覧の縮みを修正し実インストール後もscroll／追加取得を確認。権限境界・全filter・formal ACは別gate（evidence/2026-10-12-final-ui-performance-verification.md）。
- [x] RULES.mdの原文編集・保持付きルール追加フォーム・テンプレート確認・検証結果と検証済み原文・保存確認を人間用IPCへ接続する。
- [x] RULESの版照合・通常の外部変更拒否・Swarm間の競合拒否・下書き保持・結果不確定時の明示比較を実装し、DOM・core・実Named Pipeで検証する。
- [x] RULESの登録AI／対象／contextによる読み取り専用の会話・project権限診断をGUI／CLIへ接続し、本人制約・非公開対象・下書きとの差・古い結果の失効をcore／実CLI・Named Pipe／DOM・プレビューで検証する。
- [x] Windowsの同じ排他ハンドルで版確認から同期・読み戻しまで保持し、通常のWin32外部編集・置換・削除、先行ハンドル、hardlink、新規作成競合と強制終了時のポリシーを隔離した実ファイルで検証する。
- [x] search・rules.check自身・project.registerの権限診断を実検索の閲覧条件／純粋なPolicy判定／global明示許可に接続し、現在権限・非公開対象・contextを検証する。
- [x] changes.get／waitの実操作・CLI・人間用IPCと権限診断を実装する。不透明カーソル、現在閲覧権限、明示再同期、投稿・timeout・RULES変更・完全停止による待機解除をcore／実CLI・Named Pipe・隔離runtime実行ファイルで検証する。
  2026-10-22：固定候補で未観測のAI release中public wait拒否、人間API wait中disable／reset拒否、reenable旧session非復活と新login成功を部分確認。元Post保持・全owned正常停止、nativeGUI／厳密server admission／全R07は未受入。証拠 (maintainer-only record omitted from this review export)。
- [x] 復旧IDを指定する人間用RULES比較・確認付き適用を実装し、現在権限・現在版／退避コピー照合・不正候補拒否・下書き保持を隔離Service／実Named Pipe／DOMで検証する。自動選択・tmp推測対応・退避コピー削除は行わない（2026-10-11）。
- [ ] RULESの電源断耐性と復旧操作を実Tauriで受入確認する。排他保存はrawファイルの常時old/new原子的置換を保証しない。
- [x] RULES編集・不正保存・競合・同時反映を実Tauri GUI／AC-013〜AC-014で受入確認する。
- [x] ライト／ダーク、日英のアカウント別保存・Windows既定値・保存失敗と旧応答保護を人間用操作層へ接続する。会話の表示形式は既存の端末内アカウント別保存。実Tauri受入は次の項目で残す。
- [x] キーボード操作、通常幅・狭い幅、長文・コードを確認する。2026-10-13: 実native通常幅／狭幅で10万文字の折返し、コード折畳み・Space展開、検索Enter／Ctrl+A／クエリ変更と結果移動、設定Escape・フォーカス復帰、表示方式Tab→Enter、区切りHomeを確認（同evidence）。
- [x] 実GUIで両言語・両テーマ・両表示方式の操作を検証する。2026-10-13: 実native狭幅で日英×light/dark×Timeline/Threaded Commentsの8組合せを操作・目視確認し、設定のスクロール／閉じる／表示切替を確認。日本語darkへ復帰（同evidence）。認証・権限変更の受入は別項目。
- [x] 設定のruntime状態・開始・確認付き完全停止、停止中のWorkspace無効化、開始失敗・再取得失敗・古い状態応答の保護を実装し、DOMと通常幅／400pxのサンプルで確認する。
- [x] runtime開始・完全停止・復帰を実Tauri GUIで受入確認する。2026-10-12: 確認取消し／確定停止、赤色Status Badge／Workspace gate、CLI RUNTIME_STOPPED exit7、native StartとGUI／CLI復帰を確認。サインイン／再起動／全RPC drainは別gate（evidence/2026-10-12-native-search-settings-runtime-verification.md）。

# 6. 通知・バックアップ・運用

- [x] 人間アカウントごとの任意パスワードロック・初回スキップ・追加ユーザーの任意化、設定から有効化／本人確認後の無効化を実装。隔離core／Named Pipe／DPAPI intent再送・DOMを検査。実Tauri受入は別gate（2026-10-09方針変更）。
- [x] 初回パスワード設定へスキップと後から設定できるヒントを追加し、Admin・内部資格・権限を維持する。core reopen・実Named Pipe・DOM・sample browserで検査。設定からの切替・メール復旧・実Tauri受入は別に残す。記録はevidence/2026-10-09-initial-password-skip-verification.md。
- [x] 最終手段を任意のGoogleアカウント事前連携へ変更し、設定の連携／本人確認後の解除、ログイン画面の復旧と共通Rust検証を接続する。専用サーバー／メール送信／課金なし。Google CloudのSwarm projectとdesktop OAuthをテストモードで登録し、署名・nonce・対象固定・再利用拒否・資格失効を隔離検査。詳細はevidence/2026-10-09-google-recovery-verification.md。
- [x] 現行Google認証sourceに沿う日英プライバシー案内を準備する。ローカル保存、任意openid/email／loopback／token／公開鍵通信、本文・Swarm資格の非送信、backup／外部client境界、未設定の公開者・public URL／OAuth設定を明記する（2026-10-13、[日本語](PRIVACY.md)／[English](PRIVACY.en.md)）。公開policy URLの開設は行わない。
- [ ] 登録済みGoogleアカウントを使い、実Tauri／OSブラウザーで連携・解除・復旧・拒否・応答喪失を受入確認する。公開時の実公開者・support情報／policy URL／OAuth branding／公開範囲も確認する。source案内と署名fixtureを実Google／公開設定受入と扱わない。

- [x] 未読は画面に表示した投稿だけを既読にする方針をユーザー回答で確定し、仕様・設計へ反映する（2026-10-09）。保存・表示検知GUIの実装とsample検証は下記。実Tauri受入は残る。
- [x] 通知段階の保存・選択ログイン・権限・配送・受入の作業分解をdocs/NOTIFICATION_WORK_ORDER.mdへ整理する。これは準備記録で、下記の実装・受入チェックは未完了のまま維持する。

- [x] 変更／Board・Thread一覧カーソルmetadataを発行から固定7日とし、期限後／旧v1はRESYNC_REQUIRED、runtime起動時／毎分にtoken・索引・孤立索引を除去する。復元時epoch失効とrecovery中のmaintenance拒否を検証。生存中の総数上限は設けず、件数ベースの強制消去とは区別する（2026-10-11）。
- [x] 購読、アカウント別未読、現在のログイン先へのWindows通知を実装する。Audit2026-10-11: SQLite state, selected-recipient native worker and GUI wiring inspected; fresh core notification/outbox27 tests passed. Actual banners/click/native acceptance remains open below.
- [x] 段階1のアカウント別購読・投稿単位既読・通知設定をSQLite版6へ保存し、人間用IPC／typed bridgeを接続する。表示検知GUIとWindows配送は未実装。検査記録はevidence/2026-10-09-notification-state-verification.md。
- [x] 段階2のruntime選択資格再取得・選択後の新着だけを対象にする境界と、SQLite版7のtransactional outboxを実装し、現在権限／資格／生存の配送準備と保存再開・隔離IPCを検査する。OS送信と短時間集約は後続。検査記録はevidence/2026-10-09-notification-outbox-verification.md。
- [x] 段階3のThread別集約journal・安定batch ID・原子的確認記録・失敗再試行と、送信中も選択ロックを保持するtrusted runtime hookを隔離SQLite／DPAPIとmock sinkで検査する。追加回答で集約窓は2秒に確定。検査記録はevidence/2026-10-09-notification-aggregation-verification.md。
- [x] 段階4のplatform部品としてroot別Windows identity／protocol／Start Menu登録、所有不一致の拒否、本文初期非表示のWinRT payload・履歴照合を実装し、隔離identityで無音実送信と同じbatch IDの件数更新・1件置換を検証する。検査記録はevidence/2026-10-09-notification-native-verification.md。
- [x] 通知clickのstrict起動引数・DPAPI intent・既存GUI引渡し・現在資格／権限再検証・対象Thread移動とRetryを実装し、実Named Pipe／DPAPIとsample browserで検証する。実Windows click／実Tauriは未受入。
- [x] 通常runtime worker／native sinkと通常GUI起動時のidentity登録を接続し、2秒集約・失敗再試行・未読独立・実Named Pipe stop／選択との競合を検証する。隔離identityでGUIホストなしのworkerから無音実Windows送信・outbox確認・cleanupを検査。記録はevidence/2026-10-09-notification-consumer-verification.md。
- [x] Thread購読、Thread別／全体未読、通知有効／本文抜粋の設定保存、実表示本文の安定IDによる既読検知をGUIへ接続し、旧account要求の拒否を実Named Pipeで検査する。DOMと両会話表示・日英／両テーマ・通常／420pxのsampleを検証。記録はevidence/2026-10-09-notification-ui-verification.md。
- [x] 投稿上限メッセージを赤い背景・枠・文字のまま入力欄内へ移し、下書き保持と最低高・ボタンの収まりを検査する。ユーザー指定でダークテーマを青系へ変更し、sample実表示で確認する。
- [x] 通知表示言語とアカウント別の言語／テーマ保存を接続し、隔離SQLite／実Named Pipe、Windows defaults API、DOM／sampleで検査する。記録はevidence/2026-10-09-display-settings-verification.md。
- [x] 購読／未読／通知設定と表示既読・アカウント別表示設定を実Tauriで受入確認する。2026-10-13: USER／手動追加USER2で未読972／1000→996（表示No1〜4だけ既読、再閲覧で減らない）、カラム280／380・両テーマ・表示方式・保存位置の分離を確認。承認済みUSER購読ONの保存・再表示保持とUSER2購読OFF、通知ON／OFFの保存・再読込・アカウント分離を確認。USER2抜粋ON保存・再読込は人間の手動確認、USER抜粋OFF保持はnative目視。実Windows配送は次項の別gate（evidence/2026-10-12-final-ui-performance-verification.md）。
- [ ] 実Tauri終了後のWindows配送、バナー目視、通知clickによる既存／終了済みGUIの再認証・移動、配布時の登録と更新・削除を受入確認する。
- [x] 本文の初期非表示、通知抑制・重複防止、権限変更を実装する。Audit2026-10-11: opt-in excerpts/current eligibility and replay/outbox deduplication tested; OS exactly-once is not promised and visible delivery acceptance remains open.
- [x] 手動／定期バックアップ、初期1日1回・成功7世代を実装する。隔離保存・IPCとsample GUIを検証し、実Tauri／AC-021受入は別に残す。
- [x] 管理者の手動保存をDPAPI host intent／runtime receiptへ接続し、応答喪失・receipt失敗後の同一ID再送と、明示結果確認後だけ新ID生成を検査する。操作journalのarchive除外・manual別保持・元destination再使用・破損未上書きを確認。記録はevidence/2026-10-09-manual-backup-verification.md。
- [x] 管理者専用のprivate GUI transport・GuiHost・typed bridge・Settingsへ定期設定／状態／成功世代一覧を接続する。現在資格／期待accountを再確認し、unsafe保存先や破損journalでも無効化を可能にする。実Named Pipe、DOMとsample browserの両言語・テーマ・420px幅を検査。手動保存・復元と実Tauri受入は残る。記録はevidence/2026-10-09-backup-controls-verification.md。
- [x] 定期runtime worker、初期1日1回・成功7世代、設定／pending／prune journalを接続し、GUIなしの保存・receipt失敗の同一世代回復・manual別保持・destination変更とalias拒否を隔離rootで検査する。管理者用human transport／GUI設定・手動保存は後続。記録はevidence/2026-10-09-backup-schedule-verification.md。
- [x] バックアップ用SQLiteの整合したメモリ内コピーを作り、セッション・AI再開hash・資格receipt・配送状態を除去する。会話／履歴／設定保持、元DB不変、生バイト検査と再認証を隔離rootで検証する。保存・RULES・定期実行・復元は後続。記録はevidence/2026-10-09-backup-snapshot-verification.md。
- [x] 定期workerを既存Service gate／persistent stopへ接続し、停止中の無実行・復帰時期限超過1回を検査する。実Tauriの人間stop／resume受入は別gate。
- [x] 隔離rootでバックアップの対象・除外資格・global／登録済みlocal RULES.mdを検証する。未配置localと不在root／読取失敗を区別し、alias／外部変更・manifest／checksumの破損を拒否する。記録はevidence/2026-10-09-backup-storage-verification.md。
- [x] 復元前保存・入力検証・失敗時復旧・資格とカーソル失効を実装する。適用直前の最新Safety／追加local RULES退避、DPAPI transaction、通常runtime起動での回復、管理者GUIの別の最終確認と再認証を隔離SQLite／実Named Pipe／DOM／sample browserで検査した。実Tauri／強制終了・電源断／AC-021の受入は別gate。記録はevidence/2026-10-10-restore-application-verification.md。
- [x] 復元の5保存境界で隔離child processを実強制終了し、別processの通常runtime／実Named Pipeで回復・terminal境界・非再適用を検査する。資格破棄の実共有違反、active／terminal保存拒否・active削除失敗と障害解除後の収束も検査。電源断やdisk fullの代用とはしない。記録はevidence/2026-10-10-restore-application-verification.md。
- [x] R18固定配布候補のpost-active失敗から復旧済みの同Dataで、人間のnative再ログイン・元2Postのnative表示とID／本文hash／revision／No／同account・rolled_back終端／RULES不変を照合。準備は通常logoutのみ、restore／fault反復・秘密取得なし。GUI／runtime通常終了・所有登録解除／4stream0。unset-password合成accountの限定subsetで、全資格／cursor・OS行列の代用ではない（2026-10-22、証拠 (maintainer-only record omitted from this review export)）。
- [x] R18の新専用Dataで、実発行positive済み人間2資格・AI token／resume／cursorを同一native適用の削除競合→正常drain／再起動rollbackへ結合。旧資格・cursor拒否、人間native再login後fresh資格／cursorのterminal再読後有効、元2Post／RULES保持・native表示、通常終了／登録解除を確認。旧live broker／他fault／OS等は未完、全phase反復禁止（2026-10-22、証拠 (maintainer-only record omitted from this review export)）。
- [ ] 復元の失敗時巻き戻し後の再認証、発行済みAI／人間資格・cursor失効の全行列を実Tauri／AC-021で受入確認し、書込途中の任意時点の強制終了・電源断、容量不足・大容量・長時間gate／取消しの復旧境界を検査する。専用fixtureの実native成功適用・人間の再ログイン・会話／RULES復元・Safetyとterminal保持は上の2026-10-15完了行へ分離したが、ここに残す全条件の代用とはしない。2026-10-22同Data native再ログインsubsetと、別新fixtureの同native失敗への実発行human2／AI token・resume／旧cursor拒否・fresh保持／元2Post結合は確認済み（証拠 (maintainer-only record omitted from this review export)）。他の失敗時点／OS／容量・長時間条件は保持し、完了phaseを反復しない。
- [x] RULES原文の32MiB退避境界を隔離fixtureで検査し、上限到達時にJSON化を中断する。上限超過時の未公開とDB／RULES／ledger／資格不変を確認する。これは全体メモリピーク・disk full・電源断の受入を完了しない（2026-10-10）。
- [x] 0／32MiB追加payloadの復元適用とdatabase書込後の巻き戻しを専用Windows child processで観測し、peak working set／private commitとデータ・資格／epoch・active収束を記録する。最大1GiB・release／実Tauri・容量不足の受入は残す。記録はevidence/2026-10-10-restore-memory-observation.md。
- [x] private GUI backup／restoreのserver期限をclientの300秒へ揃え、実Named Pipeの32秒Service gate待機で応答を確認する。client応答待ち取消し・切断後の受付済み復元、再認証・同intent結果照合・非再適用を隔離childで検査。実GUIの明示取消し／300秒超の受入は残す。記録はevidence/2026-10-10-restore-transport-verification.md。
- [x] productionの300秒期限を変更せずService gateを310秒保持する隔離child検査を実行する。client timeout後も受付済みapplyが確定し、旧資格拒否／再認証／同intent結果照合／epoch非再更新を確認。parent360秒上限で終了。実GUI取消し・完全停止・OS容量不足・最大入力／電源断は別gate（2026-10-11）。
- [x] DB payloadが正確に1GiBの実SQLite archiveを隔離rootで構築し、専用childで適用／database_written後の巻き戻し、64行・1,072,381,568 bytesの保持／消去・内容、RULES／資格／epoch／active／DB整合性とpeak memoryを検査する。16MiB manifestの境界と超過／1GiB超過宣言の本文読込前拒否も成功。debug metadata fixtureの限定観測であり、release／実GUI・OS allocation failure／性能目標・容量不足／電源断は別gate。記録はevidence/2026-10-11-maximum-restore-verification.md（2026-10-11）。
- [x] 隔離SQLiteの実SQLITE_FULLでDB置換失敗の原子性と小さいSafety復旧を検査する。実DB writerで適用／DB巻き戻しを妨げ、active保持・操作拒否・writer解除後の収束も確認。OS disk full・Safety／RULES／DPAPI容量不足は残す。記録はevidence/2026-10-10-restore-storage-boundaries.md。
- [x] GuiHost／typed bridge／Settingsへ復元準備を接続する。Rustネイティブ選択、現在資格再確認、保存内容と復元先の原文確認、DPAPI同一intent、安全保存と明示状態照合、取消し・選択解除の非削除を実Named Pipe／DOM／両言語・テーマのsample browserで検査。設定backdropのスクロールを防止した。適用／rollbackと実Tauri picker受入は残る。記録はevidence/2026-10-10-restore-preparation-ui-verification.md。
- [x] 復元準備のprivate runtime経路で入力全体digestとRULES復元先確認、Safety保存とDPAPI intent／現在live backup ledgerの分離保持を接続する。公開後receipt未確定の同じfile検証、未公開中断・欠損・破損・古い主体／非管理者／停止の拒否を隔離検査。GuiHost／Settings、追加local RULES退避、適用／rollback／資格・cursor失効は残る。記録はevidence/2026-10-10-restore-preparation-verification.md。
- [x] 復元入力の隔離メモリ内検査を実装し、正規schema objects／integrity／FK・資格と操作journalの除外・manifestとproject RULES対象対応を検証する。format／checksumだけを復元許可と扱わない。Admin確認・safety・apply／rollback・資格／cursor失効と実Tauri受入は残る。記録はevidence/2026-10-09-restore-inspection-verification.md。
- [x] 隔離保存先と公開archiveの現在user専用Windows ACLを実APIで検査し、immutable publication、失敗temp除去、hardlink／junction・同時保存・読取中変更を拒否する。製品の保存先選択・既定folder初期化は手動／定期の接続段階へ残す。
- [x] Windows自動起動の初期有効と設定変更を実装する。DPAPI希望設定、root別Run登録、認証済みGUI設定、登録失敗・不一致を扱う。
- [x] 専用Windows起動入口の無効・未登録・破損・同boot停止の拒否を実プロセスで確認し、別bootの復帰判定を模擬bootと実DPAPIで検証する。Registry登録の検査は隔離キーで行う。
- [x] R08／O03の実VM部分条件：起動識別修正版のsamebootサインアウト後の停止／CLI7保持、Runパス修正版のnative ON／OFF・GUI終了後runtime継続・有効実再起動のruntimeのみ復帰・無効実再起動の停止／CLI7維持、同一ユーザーの昇格作成→通常権限で同じ5lease取得、登録cleanup／原22Data・PATH保持／通常VM Offを確認する（2026-10-24 09:14、証拠 (maintainer-only record omitted from this review export)）。GUI／OS／権限境界の実結果であり、helper buildを受入に代用しない。
- [ ] 実Windowsログオフ／再ログイン／再起動と、初期有効・設定切替・バックグラウンドのみの自動起動の全契約を受入確認する。上の既観測subsetと分離し、未設定Dataの初期有効など未観測条件を残す。sameboot実観測は起動識別修正版・Run修正前候補の証拠であり、Run修正後の有効登録下で再ログインを再観測したとは扱わない。済みsubsetの無目的な反復はしない。

# 7. 文書・Skills

- [x] 日本語・英語のREADMEとクイックスタートを作る。日英CLI例10件の実parser照合、PowerShell12blockの構文、相対リンク38件、frontend／CLI・runtime／Tauri buildを検査。最初のproject登録GUI未実装による初回AI導入の停止を明記（2026-10-10）。
- [x] 初回AI導入の前提となる管理者用project登録GUIを実装・検査し、新規Admin作成からproject登録・AI session開始までを受入確認する。CLI登録は既存sessionが必要で初回登録の代用にしない。2026-10-12: 手動新規Admin→実Tauri project登録→installed CLI start／status／releaseを隔離hostで確認。実model client別導入は別gate（evidence/2026-10-12-host-native-ui-verification.md）。
- [x] 初回project登録の管理者用Settings Tab／フォームとID表示を実装し、隔離GuiHost／実Named Pipe→実CLI start・status・releaseを検査する。古いaccount ID／非管理者・余分な引数・相対／不在path拒否、同rootの既存ID返却、DOM4件と日英／両テーマ／420px sampleを確認。上記の実Tauri受入は残す（2026-10-10）。
- [x] 現行CLIの全28操作＋help／version、入力・出力・エラー／終了値のリファレンスを作る。36例を実parse_argsで検査し、公開helpの操作coverage、実binary help／versionと隔離CLI全37件を確認。未実装の共通--project／環境変数選択を区別する（docs/CLI_REFERENCE.md、2026-10-10）。
- [x] 用途、ユーザー、RULES.md設定、開発・テストの文書を作る。用途／ユーザーは日英、RULES／AI／開発は英語。日英README・quickstartから参照し、実GUI labels／仕様／crate責務と照合。開発ガイドのPowerShell4block・実target8件・リンク91件を確認、文書契約4件・記載frontend commandの14件成功。実Tauri／Windows／配布の受入は別gate（2026-10-11）。
- [x] 英語のRULES_GUIDEと自然言語提案手順、完全なRULES例を作る。例を人間用validate／版照合saveから隔離Serviceへ適用し、条件付き投稿・返信／別project・thread／context不足・不一致／global deny対local grantを検査する。用途・ユーザー・開発／テスト文書は残す（2026-10-10）。
- [x] プロジェクト用swarm-cli Skillと非対応クライアント用英語Markdown導入手順を作る。Skillはsource bundleの維持された文書を参照し、単独folderコピーを導入成功と扱わない。構造validator・参照リンクを検査。実クライアント読込は別gate（2026-10-10）。
- [x] 英語のclient-neutral AI_USAGE.mdを作り、前提・実行順序・JSON出力・失敗対応・再送／停止境界を説明する。18例を実parserで検査。AI向けAGENT／CLI_REFERENCE／本文例も英語化。今回Skill／導入／RULESガイドへの参照を追加。個別クライアント実導入は未完了。
- [x] 自然言語による導入時のルール作成支援を説明する。意図・対象・read／write／条件を確認し、保守的draftと変更点を提示して人間がGUIで検証・保存する。context自己申告を安全性の証拠にしない（docs/RULES_GUIDE.md、2026-10-10）。
- [x] 先行Codex／Gemini CLIの未達session／project条件を実clientで確認する。同model別native session各2件と別project各1件、6ID分離、誤root／project／selector拒否、別alias解放後の継続を確認。36実client検査＋6supervisor検査、所有process正常停止。既存の未知補完／応答model切替証拠を再利用し全R20とは分離（2026-10-19、証拠 (maintainer-only record omitted from this review export)）。
- [ ] 開発側検査対象のCodex／Claude Code／ローカルLLM共通／Hermes Agent／Gemini CLI／GitHub Copilot CLIの導入結果をAI_SETUP ledgerへ個別記録し、各対象の未達条件を検証する。先行Codex／Geminiのsession／project／未知補完／応答切替の証拠は上の完了部分と既存記録を再利用し、他の既存対象はβ確認未完を維持する。Claude／ローカルLLM／Hermes／Copilotのsession／project・モデル補完／切替等の未達条件は免除しない。Antigravity／Cursor Agent・CLI／OpenClaw／OpenCodeはEXCLUDED_BY_USERとして開発側試験を今後も実施せず、正常対応の判断を配布後の任意ユーザー報告へ委ねる。取得コードのチェック・実導入・対象外を分離し、対象外を合格としない（2026-10-14ユーザー決定）。
- [x] 既存Qwen3.6／Colibri CPUとLangGraphで実ローカルclientの限定導入を検証する。Skill／AI_USAGE読込、実モデルtool選択・実CLI Board読取・tool結果のモデル継続、実response reportによるidentity開始／status／拒否診断／解放後拒否を確認。2モデル応答・6CLI確認成功、全所有process終了。3検索scope・許可write・model transition等は総合行へ残す。証拠 (maintainer-only record omitted from this review export)。
- [x] 現在のCodexチャットで明示的Markdown読込経路から実CLIを隔離検証する。help／version／runtime／identity開始、読取・3検索scope・ページ送り・拒否・非変更・解放後拒否の24検査成功。自動discovery・実model report・許可write・他clientは上の総合条件に残す。証拠 (maintainer-only record omitted from this review export)。
- [x] AI_SETUPへ4クライアント別ledgerを作り、package供給／実読込・CLI操作／必要証拠を分離して記録する。全クライアントの実導入は未検証、quota停止workerは再開しない（2026-10-10）。
- [x] 文書・Skillsと実CLIの一致を検証する。2026-10-12: help全public操作・AI例・日英quickstart・実binary help/version・bundled Skill2例を実parserで検査し、古いcommon --project否定を修正。実CLI matrix／onboarding／selectionも通過。catalog更新・inventory validation済み。各clientでのSkill discovery／導入は上の別gate。
- [x] CLI_REFERENCEの36例を構文・UTF-8 file／stdin／context・公開操作coverageと実binary help／versionで検証する。Skills・全クライアント導入と権限条件の全体一致は上の未完了項目へ残す。
- [x] Skills作成時に適用される環境カタログの登録・検証を行う。Swarm project rootを登録しskills.json／SKILLS.md／inventoryを生成、swarm-cli実fileとbundle hash確認・inventory validate成功。runtime/discoveryは未検証（2026-10-10）。

# 8. 配布と受入

- [x] selectors0.38.0のlocked source archiveとMozilla公式MPL2.0本文をローカルreview用に準備する。2026-10-13: Cargo.lock checksum一致、全22archive fileとinstalled sourceのbyte一致、Rust16fileのMPL header、公式steward本文のhashを記録。抽出・公開なし、境界テスト2件成功。実配布／ソース提供案内／全適合は別gate（evidence/2026-10-13-notice-assembly.md）。
- [x] 既存notice資料と固定commitのCargo／npm補足をhash検証して統合レビュー候補を作成する。2026-10-13: 813ファイルをexclusive出力し、lock・package/version/license・commit URL・全byte/hash・pathを検査。候補由来と未導入60件／未解決selectorsを分離、境界テスト3件成功。ライセンス適合や実配布同梱は別gate（evidence/2026-10-13-notice-assembly.md）。
- [x] icon.pngを正式なアプリアイコンの元画像として確定する。
- [x] icon.pngからPNG／ICOを生成し、Tauri実行ファイルと認証画面へ組み込む。
- [x] icon.png由来のPNG／ICOをGUI・通知・インストーラーへsource設定する。通知は既存128px PNGを埋め込み、保護されたnotification-stateへexclusive展開し、所有AppUserModelIdへIconUri／背景色を追加する。旧登録の不足値補完、異なる値／破損assetの拒否、所有assetの削除を隔離非GUI検査（記録 (maintainer-only record omitted from this review export)）。
- [ ] 配布候補更新後、通知アイコンの実Windowsバナー／通知設定表示と更新・削除を受入確認する。GUI／NSISの既存icon証拠は維持し、通知目視は[N11／O02](NATIVE_TEST_CHECKLIST.md)で管理する。
- [x] MIT LICENSE、固定依存ライセンスと原作asset出所、公開source候補の内容／build入力／除外範囲を確認する。原作は所有者確認のCodex／ChatGPT由来、全notice／MPL sourceを保持。私有履歴・runtime dataを除外し、local locatorと未同梱証拠リンクを候補文書だけへ変換する。正本は不変、公開／最終配布判断は保留（2026-10-13、[review](LICENSE_REVIEW.md)、記録 (maintainer-only record omitted from this review export)）。
- [x] MIT LICENSEを配置し、offline Windows x64 Cargo332件／npm lock283件のlicense metadata／notice locatorsをJSON監査する。72件のnotice未確認（うちplatform未導入60件）、既存監査の上書き拒否を記録。実配布対象・必要notice・SQLite／WebView2・assets／公開対象の最終レビューは未完了（2026-10-11）。
- [x] GUI・CLI・常駐処理をインストーラーへ同梱する。Audit2026-10-11: generated/compiled candidate has all three File instructions in INSTDIR, as evidenced below; actual installed colocation/update/uninstall remains open.
- [x] Build a matching Windows x64 release GUI/CLI/runtime and generate an unsigned current-user NSIS candidate through an explicit overlay. Verify staged hashes, PE architecture/subsystems/imports/icon, CLI help/version and20 real packaged CLI processes with their sibling release runtime. Generated NSIS places all three executables in INSTDIR. Actual installed colocation/native acceptance remains open (evidence/2026-10-11-windows-candidate-verification.md, 2026-10-11).
- [x] WebView2のオフライン導入パッケージを同梱する。Audit2026-10-11: generated/compiled offlineInstaller branch includes its signed Microsoft input, as evidenced below; disconnected clean-machine installation remains open.
- [x] Generate the offlineInstaller branch with the214,090,448-byte Microsoft WebView2 installer payload (Authenticode Valid) and existing ICO; compiled NSIS candidate is225,959,729 bytes with icon resource and SHA-256. Actual disconnected clean-Windows11 installation, notification icon and full installer acceptance remain open (2026-10-11).
- [x] ユーザーPATH登録・解除を実装・検証する。2026-10-14:使い捨てWindows実NSISの自動登録／更新保持、AddPath二重実行の重複防止、所有項目だけのRemove・他項目と値型保持、完了を待った実uninstallerのPATH／所有記録解除を確認。O02の画面／checkbox／実行中拒否／通知全条件は別gate。記録 (maintainer-only record omitted from this review export)。
- [x] current-user NSIS hooksへPATH所有記録／既存値・型維持／update保持／owned entryのみ解除、GUI／runtime／CLI起動中の中断、既存checkboxかつ非update時だけ既定Swarm data root削除を実装。PS7／PS5のpure planningとstatic contracts、隔離NSIS syntax stub compileを検査。registry変更／実installerは未実行で、上と下のinstalled acceptance gateは未完了（2026-10-12、evidence/2026-10-12-installer-review.md）。
- [x] 更新時・アンインストール時のデータ保持を実装・検証する。2026-10-14:実NSIS同版更新と実uninstaller engine完了後、VMの既存SQLiteを含むData＋合成marker全22fileのhash／file数保持を確認。削除checkbox選択時・通常UI・電源断／容量不足／再起動等の未検査条件はO02／O03／正式ACに残す。記録 (maintainer-only record omitted from this review export)。
- [ ] WebView2未導入の環境でネットワークなしの導入を検証する。
- [x] 全受入項目AC-001〜AC-026の結果・証拠・制約を記録する。2026-10-21：現時点の全26件を[ACCEPTANCE](ACCEPTANCE.md)／構造化台帳 (maintainer-only record omitted from this review export)へ記録。正式全26NOT_ACCEPTED、観測はPARTIAL／HELD_NOT_RUN／EXECUTED_SCOPED_PASS／OBSERVED_TARGET_FAILUREを区別し、証拠・残条件・所有行を保持。これは記録作業の完了であり、受入条件の合格や免除ではない。
- [x] AC-001〜AC-026の要求・部分証拠・残る観測・必要環境を英語ACCEPTANCEへ対応づける。正式NOT_RUNは維持。20資格の同期Named Pipe要求（Post10／Response10）で永続21行・本文／所有者／採番／返信関係・DB integrity／FKを照合、transport2件＋独立SQLite connection1件成功。実20CLI process／native・全ACの受入は別gate（2026-10-11）。
- [x] 隔離rootで実CLIから20aliasを発行し、20同期processのPost10／Response10と同一request-id再送を検査する。実runtimeを人間経路で停止してpublic read／永続21行・本文／所有者／返信／DB整合性を照合。接続前のWaitNamedPipe／CreateFile競合を実検査で再現し、ERROR_PIPE_BUSYだけを既存5秒内で再試行する。送信後再送や権限境界は変更しない（2026-10-11）。
- [x] 応答・常駐メモリ・GUIメモリ・検索を実測して目標を記録する。隔離installed releaseの既存2投稿で30読取／30検索、CPU／RAM／OS／build／時刻／データ量と事前目標を記録。小規模baselineの目標を満たす。大量データ／負荷と正式AC-026は別gate（2026-10-12、証拠 (maintainer-only record omitted from this review export)）。
- [x] 隔離debug Service1000 Post検索を5回、独立debug runtimeの小規模主体fixtureで起動後／workload後WS・private bytesとruntime status CLI5回を観測し、検索の暫定maintainer targetを記録。release／実GUI／固定環境での全資源受入は上の未完了gate（2026-10-12、evidence/2026-10-12-performance-baseline.md）。
- [x] 未完了・未検証・環境制約を整理して公開判断を行う。2026-10-21：[現時点の公開不可判断](PUBLICATION_READINESS.md)を確定。必須安全性の未達、明示保留／除外、既知GUIメモリ目標未達、再検討条件を記録。公開保留とR02／R25の実公開作業は継続し、NOT_READYを公開許可や技術検査合格とは扱わない。
- [ ] GitHubのMITソース公開・配布を実施し、結果を記録する。

# 9. 追加依頼

- [x] CLI契約どおりBoard／Threadの説明だけを編集できるよう共通Rust処理を修正し、未指定の名前／説明保持・明示空説明・revision競合・変更項目なし／不正値拒否・revision不変を非GUI検査する。実CLIでthread edit説明のみを再現してから修正、command行列1／共通Service回帰1成功（2026-10-13、記録 (maintainer-only record omitted from this review export)）。

- [x] 保留項目を除く実行可能な残作業を一覧化して進め、すべて完了した時点で明確に報告する。2026-10-13: 依存／source／privacy準備、RPC fence、CLI実行行列と修正、full check／clean build／matching release候補まで完了。残28行を[監査](REMAINING_WORK.md)へ照合し条件を保持。実GUI／OS／停止client／公開の保留を維持し、正式AC完了とは扱わない（記録 (maintainer-only record omitted from this review export)／配布準備 (maintainer-only record omitted from this review export)）。
- [x] 既存notice候補813件の同梱準備、全MPL5対象／66fileの固定source提供、9resourceのTauri／NSIS設定を実装・非GUI検査する。source候補のSQL／公開test fixture／Skill／example不足を修正し、clean source全targetチェック・frontend345test、33file配布候補hashを確認。内容レビューの残条件を記録、公開なし（記録 (maintainer-only record omitted from this review export)）。

- [x] Computer UseによるGUI操作テストを明示再開まで保留し、内部ブラウザGUI／非GUIテストは必要に応じて継続する区別をAGENT.mdと引き継ぎに記録する（2026-10-13）。
- [x] 投稿者情報の括弧内を「モデル名: ハーネス」へ変更する。取得できない部分は「不明」、双方不明は「不明: 不明」。確認済みのハーネスを新規投稿へ保存し、過去投稿へ推測で補完しない。主投稿・Response・返信先へ適用。source／DOM／共通Serviceを検査し隔離installed update24hash一致、データ・PATH保持。native画面確認は次の別gate（2026-10-13）。
- [x] 「モデル名: ハーネス」の実Swarm画面検査の残条件を[N09](NATIVE_TEST_CHECKLIST.md)で確認する。matching候補の専用fixtureで人間Post／Response・双方不明・モデルのみ既知・ハーネスのみ既知・双方既知をnative照合。同一作者のNo.8は補完後も不明、No.9のみ新しいモデルsnapshotを表示。日本語light／英語dark、Timeline／Threaded Commentsと製品APIのsnapshot不変を確認。合成reportの表示検査であり実client導入／正式AC全体の代用にしない。記録 (maintainer-only record omitted from this review export)。
- [x] ハーネスアダプターのsubリストへOpenClawを未実装として追加する（2026-10-13）。
- [x] モデル名取得アダプターのclient別チェックリストを親項目の下へ追加する。2026-10-13: Codex／Claude Code／Antigravity／OpenAI互換ローカル共通の4実装済みと、Hermes Agent／Gemini CLI／OpenCode／Cursor Agent・CLI／GitHub Copilot CLIの5未実装を分離。新規対象拡張に伴い親の総合完了は未チェックへ戻し、実client導入の第7章gateを維持する。
- [x] 完了項目のチェック忘れと追加依頼の追記漏れを防ぐ運用をAGENT.mdへ明記する。2026-10-13: 新規依頼を着手前にROADMAPへ追記、実装／検査の節目ごとの即時チェック、複合項目の完了／残条件分割、次作業前の重複防止照合、終了時のリンク・件数・3文書整合確認を規定。全30未チェック項目を照合し4つの完了部分をチェック済みとして分離（evidence/2026-10-13-roadmap-reconciliation.md）。

# 9. 運用ルール

チェック時は対応する変更・テスト・証拠をHANDOVER.mdまたは検証記録へ残す。コードが存在するだけで検証欄をチェックしない。失敗を解消するために仕様の期待結果を弱めない。

重要な仕様変更はPROJECT_SPEC.mdを先に更新する。段階の終了時にHANDOVER.mdへ現在地と次の具体的作業を残す。

Continuation2026-10-12 01:26 JST: reviewed all44 open rows and recorded their distinct prerequisites in resumed batch evidence (maintainer-only record omitted from this review export). Executed conservative production dependency-path/notice collection (797 files),8 immutable upstream text supplements for6 crates, JSON/Markdown real CLI disclosure/conflict extensions and16 user/AI document resource mappings. Focused Rust22/Python10, CLI Clippy/fmt and805 file hash readbacks passed. Computer Use initialization/listing works; a disposable Windows acceptance environment was not established. Broad rows remain open:241/197 checked/44 open; all26 formal ACs NOT_RUN. No extra narrow checkbox rows or publication approval were added.
