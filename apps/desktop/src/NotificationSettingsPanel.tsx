import { useEffect, useRef, useState } from 'react'
import { guiRead, guiSetNotificationSettings } from './bridge'
import type { NotificationSettings } from './types'

export default function NotificationSettingsPanel({ accountId, language, disabled }: { accountId: string; language: 'en' | 'ja'; disabled: boolean }) {
  const [saved, setSaved] = useState<NotificationSettings | null>(null)
  const [draft, setDraft] = useState<NotificationSettings | null>(null)
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState(false)
  const [retry, setRetry] = useState(0)
  const request = useRef(0)
  const ja = language === 'ja'
  useEffect(() => {
    let active = true
    setLoading(true); setError(false); setSaved(null); setDraft(null)
    void guiRead('notification.settings.get').then((value) => {
      if (active) { setSaved(value); setDraft(value) }
    }).catch(() => { if (active) setError(true) }).finally(() => { if (active) setLoading(false) })
    return () => { active = false }
  }, [accountId, retry])
  useEffect(() => {
    setSaving(false)
    return () => { request.current++ }
  }, [accountId])
  // A keyed account component and this request guard discard stale save responses.
  const save = async () => {
    if (!draft || disabled || saving) return
    const generation = ++request.current
    setSaving(true); setError(false)
    try {
      const value = await guiSetNotificationSettings(accountId, draft)
      if (generation === request.current) { setSaved(value); setDraft(value) }
    } catch { if (generation === request.current) setError(true) }
    finally { if (generation === request.current) setSaving(false) }
  }
  const changed = draft && saved && (draft.enabled !== saved.enabled || draft.include_excerpt !== saved.include_excerpt)
  return <section className="notification-settings" aria-labelledby="notification-settings-title">
    <h2 id="notification-settings-title">{ja ? '通知' : 'Notifications'}</h2>
    <p>{ja ? '選択中のログイン済みアカウントへの新着だけ通知します。未読件数は通知を無効にしても保持します。' : 'Notify only new posts for the selected signed-in account. Unread counts are kept when notifications are off.'}</p>
    {loading && <p role="status">{ja ? '読み込み中…' : 'Loading…'}</p>}
    {draft && <fieldset disabled={disabled || loading || saving}>
      <legend className="sr-only">{ja ? '通知設定' : 'Notification preferences'}</legend>
      <label><input type="checkbox" checked={draft.enabled} onChange={(event) => setDraft({ ...draft, enabled: event.target.checked })} />{ja ? 'Windows通知を有効にする' : 'Enable Windows notifications'}</label>
      <label><input type="checkbox" checked={draft.include_excerpt} onChange={(event) => setDraft({ ...draft, include_excerpt: event.target.checked })} />{ja ? '通知に本文抜粋を表示する' : 'Show a body excerpt in notifications'}</label>
    </fieldset>}
    <div className="notification-settings-actions">
      <button className="button button-primary" type="button" disabled={disabled || loading || saving || !changed || error} onClick={() => void save()}>{saving ? ja ? '保存中…' : 'Saving…' : ja ? '通知設定を保存' : 'Save notification settings'}</button>
      {error && <button className="button button-secondary" type="button" disabled={disabled || saving} onClick={() => setRetry((value) => value + 1)}>{ja ? '現在の設定を確認' : 'Check current settings'}</button>}
    </div>
    {error && <p className="workspace-error" role="alert">{ja ? '通知設定を確認できませんでした。保存に失敗した場合は現在の設定を確認してから変更してください。' : 'Could not confirm notification settings. After a failed save, check the current settings before changing them.'}</p>}
  </section>
}
