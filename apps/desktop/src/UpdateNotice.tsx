import { useEffect, useRef, useState } from 'react'
import { installUpdate, type AvailableUpdate, type UpdateProgress } from './updates'
import { assertUpdateCanStart, flushRegisteredPreferences, releaseUpdateInputLock } from './nativeClosePreferences'

export default function UpdateNotice({ update, language }: { update: AvailableUpdate; language: 'ja' | 'en' }) {
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [confirming, setConfirming] = useState(false)
  const [progress, setProgress] = useState<UpdateProgress | { phase: 'preparing' | 'restarting' } | null>(null)
  const active = useRef(true)
  const inFlight = useRef(false)
  useEffect(() => { active.current = true; return () => { active.current = false } }, [])
  const ja = language === 'ja'
  async function install() {
    if (inFlight.current) return
    inFlight.current = true
    setBusy(true)
    setConfirming(false)
    setError(null)
    setProgress({ phase: 'preparing' })
    try {
      assertUpdateCanStart()
      await flushRegisteredPreferences()
      assertUpdateCanStart()
      await installUpdate(value => { if (active.current) setProgress(value) })
      if (active.current) setProgress({ phase: 'restarting' })
    }
    catch (fault) {
      if (active.current) {
        setProgress(null)
        setError(String(fault))
      }
    }
    finally { releaseUpdateInputLock(); inFlight.current = false; if (active.current) setBusy(false) }
  }
  const percent = progress?.phase === 'downloading' && progress.total && progress.total > 0 && typeof progress.downloaded === 'number'
    ? Math.max(0, Math.min(100, Math.floor(progress.downloaded / progress.total * 100))) : undefined
  const label = progress?.phase === 'restarting' ? ja ? 'アップデート完了。再起動しています…' : 'Update complete. Restarting…'
    : progress?.phase === 'installing' ? ja ? 'インストール中…' : 'Installing…'
    : progress?.phase === 'verifying' ? ja ? 'アップデートを検証中…' : 'Verifying update…'
    : progress?.phase === 'downloading' ? ja ? 'ダウンロード中…' : 'Downloading…'
    : ja ? '更新を準備中…' : 'Preparing update…'
  return <section className="update-notice" aria-label={ja ? 'アップデート' : 'Update'}>
    <p className="update-announcement" role="status">{ja ? '最新のアップデートがあります。' : 'A new update is available.'}</p>
    <p className="update-version">{ja ? 'バージョン' : 'Version'} {update.version}</p>
    {!confirming && <button type="button" className="button button-primary" disabled={busy} onClick={() => setConfirming(true)}>{busy ? ja ? '更新を準備中…' : 'Preparing update…' : ja ? '今すぐアップデート' : 'Update now'}</button>}
    {confirming && <div className="update-confirm" role="group" aria-label={ja ? '更新の確認' : 'Confirm update'}>
      <p>{ja ? 'Swarmと常駐処理を安全に停止し、更新後に再起動します。未送信の投稿を保存してから続けてください。' : 'Swarm and its background runtime will stop safely and restart after updating. Save unsent posts before continuing.'}</p>
      <div className="update-actions"><button type="button" className="button button-primary" onClick={() => void install()}>{ja ? '更新して再起動' : 'Update and restart'}</button><button type="button" className="button" onClick={() => setConfirming(false)}>{ja ? 'キャンセル' : 'Cancel'}</button></div>
    </div>}
    {progress && <div className="update-progress" aria-live="polite">
      <progress max={100} value={progress.phase === 'restarting' ? 100 : percent} aria-label={ja ? 'アップデートの進捗' : 'Update progress'} />
      <p>{label}{percent !== undefined ? ` ${percent}%` : ''}</p>
    </div>}
    {error && <p className="update-error" role="alert">{error.includes('UPDATE_UNSAVED_WORK')
      ? ja ? '未送信の投稿または編集中の内容があります。保存・送信または編集をキャンセルしてから更新してください。' : 'Unsent posts or edits are still open. Save or send them, or cancel editing, before updating.'
      : error.includes('UPDATE_INSTALLATION_REQUIRED')
      ? ja ? 'アプリ内更新には、インストーラーで導入したSwarmを使用してください。' : 'In-app updates require an installer-based Swarm installation.'
      : error.includes('UPDATE_INSTALL_FAILED_RUNTIME_STOPPED')
        ? ja ? 'インストーラーを開始できませんでした。常駐処理は停止したままです。設定の「起動・常駐」から再開できます。' : 'The installer could not start. The runtime remains stopped; resume it in Startup & background settings.'
        : ja ? 'アップデートを開始できませんでした。接続・署名・処理中の操作を確認して再試行してください。' : 'Could not start the update. Check the connection, signature and pending operations, then retry.'}</p>}
  </section>
}
