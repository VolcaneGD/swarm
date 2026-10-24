import { useEffect, useRef, useState } from 'react'
import { guiStorageInventory, guiDeleteBackupArchive, guiClearRestoreArtifacts, type BackupArchive, type StorageInventory } from './bridge'
import './storageCleanup.css'

export default function StorageCleanupPanel({ accountId, language, disabled, onBusyChange, onCleaned }: { accountId: string; language: 'ja' | 'en'; disabled: boolean; onBusyChange?: (busy: boolean) => void; onCleaned?: () => void }) {
  const ja = language === 'ja'
  const [inventory, setInventory] = useState<StorageInventory | null>(null)
  const [review, setReview] = useState<BackupArchive | 'recovery' | null>(null)
  const [busy, setBusy] = useState(false)
  const [failed, setFailed] = useState(false)
  const [retry, setRetry] = useState(0)
  const generation = useRef(0)
  useEffect(() => {
    const current = ++generation.current
    setInventory(null); setReview(null); setFailed(false)
    void guiStorageInventory(accountId).then(value => { if (generation.current === current) setInventory(value) })
      .catch(() => { if (generation.current === current) setFailed(true) })
    return () => { generation.current++ }
  }, [accountId, retry])
  const remove = async () => {
    if (!review || busy || disabled) return
    const current = generation.current
    setBusy(true); onBusyChange?.(true); setFailed(false)
    try {
      if (review === 'recovery') await guiClearRestoreArtifacts(accountId)
      else await guiDeleteBackupArchive(accountId, review)
      if (generation.current === current) {
        setReview(null); setRetry(value => value + 1); onCleaned?.()
      }
    } catch { if (generation.current === current) setFailed(true) }
    finally { setBusy(false); onBusyChange?.(false) }
  }
  const size = (bytes: number) => {
    const unit = bytes < 1024 ? 1 : bytes < 1024 * 1024 ? 1024 : 1024 * 1024
    return `${(bytes / unit).toLocaleString(language, { maximumFractionDigits: 2 })} ${unit === 1 ? 'B' : unit === 1024 ? 'KiB' : 'MiB'}`
  }
  return <section className="storage-cleanup" aria-labelledby="storage-cleanup-title">
    <h3 id="storage-cleanup-title">{ja ? '保存済みデータの整理' : 'Manage stored copies'}</h3>
    <p>{ja ? '会話の完全削除は、以前のバックアップにあるコピーには反映されません。不要なコピーはここで個別に削除できます。定期保存は設定した世代数で整理されます。' : 'Permanently deleting conversations does not erase copies in earlier backups. Delete unwanted copies here. Scheduled backups also follow the configured generation limit.'}</p>
    <button className="button button-secondary" disabled={busy || disabled} onClick={() => setRetry(value => value + 1)}>{ja ? '保存済みデータを再確認' : 'Refresh stored copies'}</button>
    {!inventory && !failed && <p role="status">{ja ? '確認中…' : 'Loading…'}</p>}
    {inventory && <>
      {inventory.archives.length === 0 && <p>{ja ? '管理対象のバックアップはありません。' : 'No managed backups.'}</p>}
      <ul className="storage-copy-list">{inventory.archives.map(item => <li key={`${item.directory}/${item.key}`}>
        <div><code>{item.key}</code><br /><span>{item.directory}</span><br /><span>{size(item.bytes)}</span></div>
        <button className="button button-secondary" disabled={disabled || busy || !item.deletable} onClick={() => setReview(item)}>{ja ? '完全削除' : 'Permanently delete'}</button>
        {!item.deletable && <span>{ja ? '保存処理中' : 'Save in progress'}</span>}
      </li>)}</ul>
      <p>{ja ? `復元準備・安全コピー：${size(inventory.recovery_bytes)}` : `Restore preparation and safety copies: ${size(inventory.recovery_bytes)}`}</p>
      <p>{ja ? '以下の操作は全アカウントの復元準備・完了履歴・安全コピーを破棄します。進行中または復旧待ちの復元は保護されます。現在の会話・RULES・認証情報は保持します。' : 'The action below discards restore preparations, completed receipts and safety copies for all accounts. Active or interrupted restoration is protected. Current conversations, RULES and credentials remain.'}</p>
      <button className="button button-secondary" disabled={disabled || busy} onClick={() => setReview('recovery')}>{ja ? '復元用の一時データを削除' : 'Delete restore artifacts'}</button>
    </>}
    {review && <section role="alertdialog" aria-modal="false" aria-labelledby="storage-delete-title" className="storage-delete-review">
      <h4 id="storage-delete-title">{ja ? '完全削除の確認' : 'Confirm permanent deletion'}</h4>
      <p>{review === 'recovery' ? ja ? 'すべての復元準備・完了履歴・安全コピーを完全削除します。元に戻せません。' : 'Permanently delete all restore preparations, completed receipts and safety copies. This cannot be undone.' : <><code>{review.key}</code><br />{review.directory}<br />{ja ? 'このバックアップからは復元できなくなります。' : 'This backup will no longer be available for restoration.'}</>}</p>
      <div className="storage-copy-actions"><button autoFocus className="button button-secondary" disabled={busy} onClick={() => setReview(null)}>{ja ? 'キャンセル' : 'Cancel'}</button><button className="button button-primary" disabled={busy || disabled} onClick={() => void remove()}>{busy ? ja ? '削除中…' : 'Deleting…' : ja ? '完全削除を実行' : 'Delete permanently'}</button></div>
    </section>}
    {failed && <p role="alert" className="workspace-error">{ja ? 'データを確認・削除できませんでした。保存先、権限、処理状態を再確認してください。失敗した操作は完了と見なされません。' : 'Could not verify or delete stored data. Check storage, permissions and active operations, then refresh. A failed request is not treated as completed.'}</p>}
  </section>
}
