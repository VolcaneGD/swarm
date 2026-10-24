import { useEffect, useRef, useState } from 'react'
import { guiRetentionSettings, guiSetRetentionSettings, type RetentionSettings } from './bridge'

export default function RetentionSettingsPanel({ accountId, language, disabled, onBusyChange }: { accountId: string; language: 'en' | 'ja'; disabled: boolean; onBusyChange?: (busy: boolean) => void }) {
  const [settings, setSettings] = useState<RetentionSettings | null>(null)
  const [history, setHistory] = useState('')
  const [deleted, setDeleted] = useState('')
  const [deletedBoards, setDeletedBoards] = useState('')
  const [deletedThreads, setDeletedThreads] = useState('')
  const [archivedThreads, setArchivedThreads] = useState('')
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<'read' | 'save' | 'denied' | null>(null)
  const [retry, setRetry] = useState(0)
  const generation = useRef(0)
  const running = useRef(false)
  const ja = language === 'ja'
  useEffect(() => {
    const current = ++generation.current
    setLoading(true); setSettings(null); setError(null)
    void guiRetentionSettings(accountId).then(value => {
      if (current !== generation.current) return
      setSettings(value); setHistory(value.history_days == null ? '' : String(value.history_days)); setDeleted(value.deleted_days == null ? '' : String(value.deleted_days))
      setDeletedBoards(value.deleted_board_days == null ? '' : String(value.deleted_board_days)); setDeletedThreads(value.deleted_thread_days == null ? '' : String(value.deleted_thread_days)); setArchivedThreads(value.archived_thread_days == null ? '' : String(value.archived_thread_days))
    }).catch(fault => { if (current === generation.current) setError(fault?.code === 'PERMISSION_DENIED' ? 'denied' : 'read') })
      .finally(() => { if (current === generation.current) setLoading(false) })
    return () => { generation.current++ }
  }, [accountId, retry])
  const days = (value: string) => value === '' ? null : Number(value)
  const retentionDraft = [history, deleted, deletedBoards, deletedThreads, archivedThreads]
  const valid = retentionDraft.every(value => value === '' || (/^\d+$/.test(value) && Number.isSafeInteger(Number(value)) && Number(value) > 0))
  const draft = {
    history_days: days(history), deleted_days: days(deleted),
    ...(settings?.deleted_board_days !== undefined ? { deleted_board_days: days(deletedBoards) } : {}),
    ...(settings?.deleted_thread_days !== undefined ? { deleted_thread_days: days(deletedThreads) } : {}),
    ...(settings?.archived_thread_days !== undefined ? { archived_thread_days: days(archivedThreads) } : {}),
  }
  const changed = settings && (draft.history_days !== settings.history_days || draft.deleted_days !== settings.deleted_days
    || (settings.deleted_board_days !== undefined && draft.deleted_board_days !== settings.deleted_board_days)
    || (settings.deleted_thread_days !== undefined && draft.deleted_thread_days !== settings.deleted_thread_days)
    || (settings.archived_thread_days !== undefined && draft.archived_thread_days !== settings.archived_thread_days))
  const save = async () => {
    if (!valid || !changed || disabled || loading || running.current || error) return
    running.current = true; setSaving(true); onBusyChange?.(true)
    const current = generation.current
    try {
      const value = await guiSetRetentionSettings(accountId, draft)
      if (current === generation.current) setSettings(value)
    } catch { if (current === generation.current) setError('save') }
    finally { running.current = false; if (current === generation.current) setSaving(false); onBusyChange?.(false) }
  }
  return <section className="backup-settings" aria-labelledby="retention-title">
    <h2 id="retention-title">{ja ? '履歴・削除済みデータの保持' : 'History and deleted data retention'}</h2>
    <p>{ja ? '初期設定は無期限です。空欄は無期限、日数を指定すると期限を過ぎた内容を実行中に定期消去します。完全削除した内容は復元できません。投稿番号と返信関係は再利用されません。' : 'The default is indefinite. Leave a field blank for indefinite retention. While Swarm runs, expired content is permanently removed. Purged content cannot be restored, and Post numbers and reply relationships are never reused.'}</p>
    <p>{ja ? '削除済みBoard・Threadの保持期間は、それぞれの削除日時から数えます。過去スレの保持期間は、Threadが1,000件の投稿に達してアーカイブされた時点から数えます。' : 'Deleted Board and Thread ages start at deletion. Archived Thread age starts when the Thread reaches 1,000 Posts and becomes archived.'}</p>
    <p>{ja ? 'バックアップ内のコピーは別管理です。「バックアップ」タブから、不要なバックアップと復元用の一時データを削除できます。' : 'Backup copies are managed separately. Use the Backup tab to delete unwanted backups and restore artifacts.'}</p>
    {loading && <p role="status">{ja ? '読み込み中…' : 'Loading…'}</p>}
    {settings && <fieldset disabled={disabled || loading || saving || error !== null}>
      <legend className="sr-only">{ja ? '保持期間' : 'Retention periods'}</legend>
      <label>{ja ? '編集履歴の保持日数' : 'History retention (days)'}<input inputMode="numeric" type="text" value={history} placeholder={ja ? '無期限' : 'Indefinite'} onChange={event => setHistory(event.target.value)} /></label>
      <label>{ja ? '削除済み本文の保持日数' : 'Deleted body retention (days)'}<input inputMode="numeric" type="text" value={deleted} placeholder={ja ? '無期限' : 'Indefinite'} onChange={event => setDeleted(event.target.value)} /></label>
      <label>{ja ? '削除済みBoardの保持日数' : 'Deleted Board retention (days)'}<input inputMode="numeric" type="text" value={deletedBoards} placeholder={ja ? '無期限' : 'Indefinite'} onChange={event => setDeletedBoards(event.target.value)} /></label>
      <label>{ja ? '削除済みThreadの保持日数' : 'Deleted Thread retention (days)'}<input inputMode="numeric" type="text" value={deletedThreads} placeholder={ja ? '無期限' : 'Indefinite'} onChange={event => setDeletedThreads(event.target.value)} /></label>
      <label>{ja ? '過去スレの保持日数' : 'Archived Thread retention (days)'}<input inputMode="numeric" type="text" value={archivedThreads} placeholder={ja ? '無期限' : 'Indefinite'} onChange={event => setArchivedThreads(event.target.value)} /></label>
    </fieldset>}
    <div className="notification-settings-actions">
      <button className="button button-primary" type="button" disabled={disabled || loading || saving || error !== null || !valid || !changed} onClick={() => void save()}>{saving ? ja ? '保存中…' : 'Saving…' : ja ? '保持設定を保存' : 'Save retention settings'}</button>
      <button className="button button-secondary" type="button" disabled={disabled || loading || saving} onClick={() => setRetry(value => value + 1)}>{ja ? '現在の設定を確認' : 'Check current settings'}</button>
    </div>
    {error && <p className="workspace-error" role="alert">{error === 'denied' ? ja ? '保持設定を管理する権限がありません。' : 'You do not have permission to manage retention.' : error === 'save' ? ja ? '保存結果を確認できません。現在の設定を確認してください。' : 'Save result is uncertain. Check current settings before continuing.' : ja ? '保持設定を確認できません。' : 'Could not confirm retention settings.'}</p>}
  </section>
}
