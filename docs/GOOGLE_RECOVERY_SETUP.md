# Googleアカウントによる最終復旧

初回βではGoogle連携・Google復旧を無効化しています。GUIと実行経路の両方で利用できません。通常のパスワード認証・復旧コード・バックアップは維持します。以下は将来の再有効化に向けた開発時の構成記録であり、現βで利用できる手順ではありません。一般利用向け公開設定と実Google検証は後日行います。

2026-10-09のユーザー承認により、復旧コードを第一手段、任意のGoogleアカウント事前連携を最終手段とする。運営側の送信サーバー・送信料・有料Cloudサービスを追加しない。

## 登録済み構成

- Google Cloud project: `Swarm` / `swarm-511113` / number `1082621526558`。
- Desktop OAuth client: `Swarm Windows`。
- 公開クライアントID: `1082621526558-3sp83c7bf85cu9bfvikv75bh925qcngb.apps.googleusercontent.com`。
- Google Auth Platform: 外部・テスト中、現在のログインアカウントをテストユーザーへ登録済み。
- スコープ: `openid`、`https://www.googleapis.com/auth/userinfo.email`。メール送受信・profile・Cloud操作の権限を要求しない。
- サポート連絡先とGoogleからの管理連絡先は登録時のログインアカウントを使用した。
- 課金アカウント接続・有料リソース作成・アプリ公開は行っていない。プロジェクト作成に伴う既定APIの存在と、Swarmが要求するOAuthスコープを区別する。
- クライアントのシークレットをソースへ保存せず、Google公式native flowの任意client_secretを送信しない。実Googleでのtoken交換は受入未完了で、登録したdesktop clientで要確認。

## 利用

設定の「Googleアカウントによる復旧（任意）」で連携する。ロック有効なら現在のSwarmパスワードが必要。OSの既定ブラウザーでGoogleへログインし、選んだアカウントを確認する。変更したい場合は現在の連携を解除してから再連携する。

パスワードと復旧コードを失った場合、ログイン画面の「Googleアカウントで復旧」へSwarmアカウント名と新しいパスワードを入力し、事前連携したGoogleアカウントで認証する。Adminの旧復旧コードと旧人間ログイン資格は失効する。再設定後は新しい復旧コードを別途保存する。

Googleブラウザーの既存ログインが利用される場合がある。SwarmがGoogleパスワードの毎回入力やMFAを強制する機能ではない。GoogleアカウントとOSのログイン状態を保護する。全ての復旧手段を失った場合の迂回解除は設けない。

## 実装境界

Authorization Code + PKCE S256、ランダムstateとruntime nonce、127.0.0.1の動的port callbackを使う。認証待ちは最大240秒、token交換15秒、公開鍵取得10秒。キャンセル・timeoutは自動的に新しい認証を開始しない。認証中は共通GuiHost mutexを保持するため、他のhost操作は待機する。待機途中のGUI取消しとprocess再起動からの認証継続は未実装。

署名検証を伴う連携完了と復旧発行のprivate RPCは30秒待つ。RPC timeoutはruntime処理の取消しではない。復旧は既存pendingの同一intentを使い、連携は公開状態を再取得して確認する。

runtimeのchallengeは1アカウント1件・5分。新しいbeginは前のchallengeを置換する。対象・用途・パスワードと連携の版を検査し、署名の検証完了後にも期限を確認する。subのハッシュと表示用の確認済みアドレスだけを保存し、refresh token・メール権限は保持しない。Google tokenは通常メモリ内だけで扱う。パスワード復旧を送信する直前だけ、同一intentのDPAPI pending-authへID tokenと新パスワードを保存し、既存の資格receipt／再送を使う。成功・確定拒否・明示破棄でpendingを除去する。Googleとのcode交換後にprocessが停止し、まだpending保存前なら改めて認証する。

Google連携とchallengeはbackup snapshotから除外する。復元入力に残っていれば拒否し、古い連携を復活させない。通常session・復旧コードの扱いは既存仕様を維持する。

## 公開前の残作業

実TauriとOSブラウザーでのGoogle連携／解除／復旧、登録直後の反映遅延、取消し・通信失敗・別アカウント・応答喪失を検査する。公開用のプライバシーポリシーURL・brandingと公開範囲を整えるまで、テスト中を一般配布済みと扱わない。有料サービスの追加や課金接続を自動で行わない。

根拠: [Google native OAuth](https://developers.google.com/identity/protocols/oauth2/native-app)、[OpenID Connect](https://developers.google.com/identity/openid-connect/openid-connect)、[Googleユーザーデータポリシー](https://developers.google.com/terms/api-services-user-data-policy)。
