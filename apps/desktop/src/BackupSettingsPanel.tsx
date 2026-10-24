import { useCallback, useEffect, useRef, useState } from 'react'
import RestorePreparationPanel from './RestorePreparationPanel'
import StorageCleanupPanel from './StorageCleanupPanel'
import { guiBackupStatus, guiSetBackupSettings, guiCreateManualBackup, guiAckManualBackup, type BackupSettings, type BackupStatus, type RestoreOutcome } from './bridge'

export default function BackupSettingsPanel({ accountId, language, disabled: externallyDisabled, onBusyChange, onRestoreFinished }: { accountId: string; language: 'en' | 'ja'; disabled: boolean; onBusyChange?: (busy: boolean) => void; onRestoreFinished?: (outcome: RestoreOutcome) => void }) {
  const [status, setStatus] = useState<BackupStatus | null>(null)
  const [draft, setDraft] = useState<BackupSettings | null>(null)
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [failed, setFailed] = useState(false)
  const [retry, setRetry] = useState(0)
  const [manualFailed, setManualFailed] = useState(false)
  const [restoreBusy, setRestoreBusy] = useState(false)
  const [storageBusy, setStorageBusy] = useState(false)
  const disabled = externallyDisabled || storageBusy
  const [storageRevision, setStorageRevision] = useState(0)
  const storageBusyChange = useCallback((next: boolean) => { setStorageBusy(next); onBusyChange?.(next) }, [onBusyChange])
  const restoreBusyChange = useCallback((next: boolean) => { setRestoreBusy(next); onBusyChange?.(next) }, [onBusyChange])
  const generation = useRef(0)
  const ja = language === 'ja'
  useEffect(() => {
    const current = ++generation.current
    setLoading(true); setSaving(false); setFailed(false); setManualFailed(false); setStatus(null); setDraft(null)
    void guiBackupStatus(accountId).then(value => {
      if (current === generation.current) { setStatus(value); setDraft(value.settings) }
    }).catch(() => { if (current === generation.current) setFailed(true) })
      .finally(() => { if (current === generation.current) setLoading(false) })
    return () => { generation.current++ }
  }, [accountId, retry])
  const save = async () => {
    if (!draft || disabled || saving || restoreBusy || storageBusy || failed) return
    const current = ++generation.current
    setSaving(true)
    try {
      const value = await guiSetBackupSettings(accountId, draft)
      if (current === generation.current) { setDraft(value); setStatus(s => s ? { ...s, settings: value } : s) }
    } catch { if (current === generation.current) setFailed(true) }
    finally { if (current === generation.current) setSaving(false) }
  }
  const changed = status && draft && JSON.stringify(status.settings) !== JSON.stringify(draft)
  const manual = status?.manual
  const runManual = async (ack: boolean) => {
    if (disabled || saving || restoreBusy || storageBusy || loading || failed) return
    const current = ++generation.current
    setSaving(true); setManualFailed(false)
    try {
      if (ack && manual) {
        await guiAckManualBackup(accountId, manual.intent)
        if (current === generation.current) setStatus(s => s ? { ...s, manual: null } : s)
      } else {
        const result = await guiCreateManualBackup(accountId)
        if (current === generation.current) setStatus(s => s ? { ...s, manual: result } : s)
      }
    } catch { if (current === generation.current) setManualFailed(true) }
    finally { if (current === generation.current) setSaving(false) }
  }
  const valid = draft && Number.isInteger(draft.interval_seconds) && draft.interval_seconds >= 60 && draft.interval_seconds <= 31536000 && Number.isInteger(draft.retention) && draft.retention >= 1 && draft.retention <= 100
  const schedule = status?.schedule
  return <section className="backup-settings" aria-labelledby="backup-settings-title">
    <h2 id="backup-settings-title">{ja ? 'バックアップ' : 'Backups'}</h2>
    <p>{ja ? '会話・アカウント情報・RULESに加え、各種設定、ウィンドウのサイズと位置、メニュー幅や会話の表示設定も保存します。' : 'Backups include conversations, accounts and RULES, plus application settings, window size and position, menu widths and conversation display preferences.'}</p>
    <p>{ja ? '定期保存は初期有効、1日1回、成功した直近7世代です。完全停止中は保存せず、復帰時に期限を過ぎていれば1回保存します。' : 'Scheduled backups default to once a day and the latest 7 successful generations. After a full stop, an overdue backup runs once on resume.'}</p>
    {loading && <p role="status">{ja ? '読み込み中…' : 'Loading…'}</p>}
    {draft && <fieldset disabled={disabled || loading || saving || restoreBusy || storageBusy || failed}>
      <legend className="sr-only">{ja ? '定期バックアップ設定' : 'Scheduled backup settings'}</legend>
      <label className="backup-enabled"><input type="checkbox" checked={draft.enabled} onChange={e => setDraft({ ...draft, enabled: e.target.checked })} />{ja ? '定期バックアップを有効にする' : 'Enable scheduled backups'}</label>
      <label>{ja ? '保存間隔（秒）' : 'Interval (seconds)'}<input type="number" min="60" max="31536000" step="1" value={draft.interval_seconds} onChange={e => setDraft({ ...draft, interval_seconds: Number(e.target.value) })} /></label>
      <label>{ja ? '成功世代の保持数' : 'Successful generations to retain'}<input name="retention" type="number" min="1" max="100" step="1" value={draft.retention} onChange={e => setDraft({ ...draft, retention: Number(e.target.value) })} /></label>
      <label>{ja ? 'ローカル保存先（絶対パス）' : 'Local destination (absolute path)'}<input type="text" maxLength={32768} value={draft.destination ?? ''} placeholder={status?.default_destination} onChange={e => setDraft({ ...draft, destination: e.target.value || null })} /></label>
      <p className="backup-note">{ja ? '空欄は既定の保存先を使用します。既存フォルダーのアクセス権は自動変更しません。' : 'Leave blank for the default destination. Existing folder permissions are never changed automatically.'}</p>
    </fieldset>}
    <div className="notification-settings-actions">
      <button className="button button-primary" disabled={disabled || loading || saving || restoreBusy || failed || !changed || !valid} onClick={() => void save()}>{saving ? ja ? '保存中…' : 'Saving…' : ja ? 'バックアップ設定を保存' : 'Save backup settings'}</button>
      <button className="button button-secondary" disabled={disabled || loading || saving || restoreBusy} onClick={() => setRetry(n => n + 1)}>{ja ? '現在の設定を確認' : 'Check current settings'}</button>
    </div>
    {failed && <p className="workspace-error" role="alert">{ja ? '設定を確認できませんでした。現在の設定を確認してから変更してください。' : 'Could not confirm settings. Check current settings before making changes.'}</p>}
    {status?.journal_error && <p className="workspace-error" role="alert">{ja ? '履歴を確認できません。履歴は保持されています。定期保存の無効化は可能です。' : 'Backup history could not be verified and has been preserved. Scheduling can still be disabled.'}</p>}
    <section className="backup-history" aria-label={ja ? '手動バックアップ' : 'Manual backup'}>
      <h3>{ja ? '手動バックアップ' : 'Manual backup'}</h3>
      <p>{ja ? '保存済みの設定にある保存先を使用します。手動分は定期保存の世代削除に含めません。結果を確認してから次の保存を開始できます。' : 'Uses the saved destination setting. Manual backups are kept separately from scheduled retention. Confirm the result before starting another save.'}</p>
      <div className="notification-settings-actions">
        <button className="button button-primary" disabled={disabled || loading || saving || restoreBusy || failed || Boolean(changed) || manual?.state === 'completed'} onClick={() => void runManual(false)}>{manual || manualFailed ? ja ? '同じバックアップを再試行' : 'Retry the same backup' : ja ? '手動バックアップを保存' : 'Save a manual backup'}</button>
        {manual?.state === 'completed' && <button className="button button-secondary" disabled={disabled || saving || restoreBusy} onClick={() => void runManual(true)}>{ja ? '保存結果を確認済みにする' : 'Confirm saved result'}</button>}
      </div>
      {manualFailed && <p className="workspace-error" role="alert">{ja ? '保存または結果確認を完了できませんでした。同じ処理を再試行するか「現在の設定を確認」で結果を再確認してください。' : 'Could not confirm the save or acknowledgement. Retry the same operation or use Check current settings to confirm its result.'}</p>}
      {manual && <p role="status">{manual.state === 'completed' ? ja ? '手動バックアップを保存しました。' : 'Manual backup saved.' : ja ? '未確認の保存処理があります。再試行で同じ処理を確認できます。' : 'A save remains unconfirmed. Retry to check the same operation.'}</p>}
      {manual?.key && <p><code>{manual.key}</code><br />{manual.destination}</p>}
    </section>
    {schedule && <div className="backup-history">
      <p>{ja ? '前回成功：' : 'Last success: '}{schedule.last_success_at === null ? ja ? 'まだありません' : 'None yet' : new Date(schedule.last_success_at * 1000).toLocaleString(language)}</p>
      <p>{ja ? '保持中の成功世代：' : 'Retained successful generations: '}{schedule.generations.length}</p>
      {status.pending_stage === 'publication_pending' && <p role="status">{ja ? '保存ファイルの作成を待っています。' : 'Waiting for the backup file to be published.'}</p>}
      {status.pending_stage === 'receipt_pending' && <p role="status">{ja ? '保存ファイルは作成済みです。成功履歴への記録を待っています。' : 'The backup file is published. Recording its success is pending.'}</p>}
      {status.pending_stage === 'unavailable' && <p className="workspace-error" role="alert">{ja ? '処理待ちの保存先またはファイルを確認できません。前回の成功履歴は保持されています。' : 'The pending destination or file could not be verified. Previous success records are preserved.'}</p>}
      {schedule.prune.length > 0 && <p role="status">{ja ? `古い成功世代の削除待ち：${schedule.prune.length}件` : `Older successful generations awaiting removal: ${schedule.prune.length}`}</p>}
      {schedule.generations.length > 0 && <details><summary>{ja ? '定期バックアップの世代一覧' : 'Scheduled backup generations'}</summary><ul>{schedule.generations.map(g => <li key={g.key}><code>{g.key}</code><br />{g.destination}</li>)}</ul></details>}
    </div>}
    <StorageCleanupPanel accountId={accountId} language={language} disabled={disabled || loading || saving || restoreBusy} onBusyChange={storageBusyChange} onCleaned={() => { setStorageRevision(value => value + 1); setRetry(value => value + 1) }} />
    <RestorePreparationPanel key={storageRevision} accountId={accountId} language={language} disabled={disabled || loading || saving || storageBusy} onBusyChange={restoreBusyChange} onRestoreFinished={onRestoreFinished} />
  </section>
}
