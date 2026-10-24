import { useEffect, useRef, useState } from 'react'
import { installUpdate, restartAfterUpdate, type AvailableUpdate, type UpdateProgress } from './updates'

export default function UpdateNotice({ update, language }: { update: AvailableUpdate; language: 'ja' | 'en' }) {
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<'unconfigured' | 'failed' | null>(null)
  const [progress, setProgress] = useState<UpdateProgress | { phase: 'preparing' | 'restarting' } | null>(null)
  const active = useRef(true)
  const inFlight = useRef(false)
  useEffect(() => { active.current = true; return () => { active.current = false } }, [])
  const ja = language === 'ja'
  async function install() {
    if (inFlight.current) return
    inFlight.current = true
    setBusy(true)
    setError(null)
    setProgress({ phase: 'preparing' })
    try {
      await installUpdate(value => { if (active.current) setProgress(value) })
      if (active.current) setProgress({ phase: 'restarting' })
      await restartAfterUpdate()
    }
    catch (fault) {
      if (active.current) {
        setProgress(null)
        setError(String(fault).includes('UPDATE_NOT_CONFIGURED') ? 'unconfigured' : 'failed')
      }
    }
    finally { inFlight.current = false; if (active.current) setBusy(false) }
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
    <button type="button" className="button button-primary" disabled={busy} onClick={() => void install()}>{busy ? ja ? '更新を準備中…' : 'Preparing update…' : ja ? '今すぐアップデート' : 'Update now'}</button>
    {progress && <div className="update-progress" aria-live="polite">
      <progress max={100} value={progress.phase === 'restarting' ? 100 : percent} aria-label={ja ? 'アップデートの進捗' : 'Update progress'} />
      <p>{label}{percent !== undefined ? ` ${percent}%` : ''}</p>
    </div>}
    {error && <p className="update-error" role="alert">{error === 'unconfigured'
      ? ja ? 'アップデートの配信設定は公開後に有効になります。' : 'Update delivery will be enabled after publication.'
      : ja ? 'アップデートを開始できませんでした。もう一度お試しください。' : 'Could not start the update. Please try again.'}</p>}
  </section>
}
