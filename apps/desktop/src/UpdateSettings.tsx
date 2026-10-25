import { useEffect, useState } from 'react'
import { getVersion } from '@tauri-apps/api/app'
import { checkForUpdates, useStartupUpdate } from './updates'
import UpdateNotice from './UpdateNotice'

export default function UpdateSettings({ language }: { language: 'ja' | 'en' }) {
  const ja = language === 'ja'
  const update = useStartupUpdate()
  const [version, setVersion] = useState<string | null>(null)
  useEffect(() => {
    let active = true
    void getVersion().then(value => { if (active) setVersion(value) }).catch(() => { if (active) setVersion('') })
    return () => { active = false }
  }, [])
  const [status, setStatus] = useState<'idle' | 'checking' | 'latest' | 'unpublished' | 'failed'>('idle')
  async function check() {
    if (status === 'checking') return
    setStatus('checking')
    try { const result = await checkForUpdates(); setStatus(result ? 'idle' : 'latest') }
    catch (error) { setStatus(String(error).includes('UPDATE_UNPUBLISHED') ? 'unpublished' : 'failed') }
  }
  return <section className="settings-preferences update-settings" aria-label={ja ? 'アプリの更新' : 'App updates'}>
    <h3>{ja ? 'アプリの更新' : 'App updates'}</h3>
    <p>{ja ? '現在のバージョン：' : 'Current version: '}<strong>{version ?? (ja ? '取得中…' : 'Loading…')}</strong>{version === '' && (ja ? '取得できませんでした' : 'Unavailable')}</p>
    <p>{ja ? 'GitHub Releasesで公開された更新を確認します。インストールは確認後に実行します。' : 'Check updates published on GitHub Releases. Installation starts only after your confirmation.'}</p>
    <button className="button" type="button" disabled={status === 'checking'} onClick={() => void check()}>{status === 'checking' ? ja ? '確認中…' : 'Checking…' : ja ? '更新を確認' : 'Check for updates'}</button>
    {status === 'latest' && <p role="status">{ja ? '現在のバージョンは最新です。' : 'You are up to date.'}</p>}
    {status === 'unpublished' && <p role="status">{ja ? '公開済みの更新情報が見つかりません。リポジトリの公開・配布開始後に利用できます。' : 'No public update feed is available. Updates become available after public distribution starts.'}</p>}
    {status === 'failed' && <p role="alert">{ja ? '更新情報を取得できませんでした。接続を確認して再試行してください。' : 'Could not check for updates. Check your connection and retry.'}</p>}
    {update && <UpdateNotice update={update} language={language} />}
  </section>
}
