import { useEffect, useRef, useState } from 'react'
import { guiRestoreStatus, guiPickRestoreBackup, guiPrepareRestore, guiClearRestore, guiApplyRestore, type RestoreStatus, type RestoreOutcome } from './bridge'

export default function RestorePreparationPanel({ accountId, language, disabled, onBusyChange, onRestoreFinished }: { accountId: string; language: 'en' | 'ja'; disabled: boolean; onBusyChange: (busy: boolean) => void; onRestoreFinished?: (outcome: RestoreOutcome) => void }) {
  const [status, setStatus] = useState<RestoreStatus | null>(null)
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState(false)
  const [failed, setFailed] = useState(false)
  const [confirmed, setConfirmed] = useState(false)
  const [finalReview, setFinalReview] = useState(false)
  const [applyUncertain, setApplyUncertain] = useState(false)
  const [outcome, setOutcome] = useState<RestoreOutcome | null>(null)
  const generation = useRef(0)
  const inFlight = useRef(false)
  const ja = language === 'ja'
  useEffect(() => {
    const current = ++generation.current
    inFlight.current = false; setBusy(false); onBusyChange(false)
    setLoading(true); setStatus(null); setFailed(false); setConfirmed(false)
    setFinalReview(false); setApplyUncertain(false); setOutcome(null)
    void guiRestoreStatus(accountId).then(value => { if (current === generation.current) setStatus(value) })
      .catch(() => { if (current === generation.current) setFailed(true) })
      .finally(() => { if (current === generation.current) setLoading(false) })
    return () => { generation.current++; onBusyChange(false) }
  }, [accountId, onBusyChange])
  const run = async (action: 'choose' | 'prepare' | 'check' | 'clear') => {
    if (disabled || loading || inFlight.current || (action === 'prepare' && (!confirmed || failed || !status?.preview))) return
    inFlight.current = true
    const current = ++generation.current
    setBusy(true); onBusyChange(true); setFailed(false)
    try {
      const value = action === 'choose' ? await guiPickRestoreBackup(accountId) : action === 'prepare' ? await guiPrepareRestore(accountId) : action === 'clear' ? await guiClearRestore(accountId) : await guiRestoreStatus(accountId)
      if (current === generation.current && !(action === 'choose' && value === null)) { setStatus(value); setConfirmed(false) }
    } catch { if (current === generation.current) { setFailed(true); setConfirmed(false) } }
    finally { if (current === generation.current) { inFlight.current = false; setBusy(false); onBusyChange(false) } }
  }
  const blocked = disabled || loading || busy
  const terminal = status?.state === 'applied' || status?.state === 'rolled_back' || outcome !== null
  const apply = async () => {
    if (blocked || inFlight.current || failed || !confirmed || !finalReview || status?.state !== 'prepared' || applyUncertain || terminal) return
    inFlight.current = true
    const current = ++generation.current
    setBusy(true); onBusyChange(true)
    let result: RestoreOutcome = 'uncertain'
    try {
      const value = await guiApplyRestore(accountId)
      if (value.reauthentication_required !== true || !['applied', 'rolled_back'].includes(value.state)) throw new Error('Invalid restore response')
      result = value.state
    } catch { if (current === generation.current) setApplyUncertain(true) }
    finally {
      if (current === generation.current) {
        setOutcome(result); setConfirmed(false); setFinalReview(false)
        inFlight.current = false; setBusy(false); onBusyChange(false)
        onRestoreFinished?.(result)
      }
    }
  }
  const preview = status?.preview
  const unavailable = status?.state === 'unavailable' && !status.can_retry
  return <details className="restore-preparation">
    <summary>{ja ? 'バックアップからの復元準備' : 'Prepare a restore from backup'}</summary>
    <div className="restore-preparation-body">
      <p>{ja ? 'バックアップの内容と復元先を確認して安全保存します。その後、別の最終確認で復元を適用できます。適用直前の状態も保存し、失敗時は巻き戻します。' : 'Review the backup contents and destinations, then save for safety. A separate final confirmation applies the restore. The state immediately before application is also saved for rollback on failure.'}</p>
      <div className="notification-settings-actions">
        <button className="button button-secondary" disabled={blocked || finalReview || terminal || applyUncertain || Boolean(status?.preparation) || status?.state === 'unavailable'} onClick={() => void run('choose')}>{ja ? 'バックアップファイルを選択' : 'Choose backup file'}</button>
        <button className="button button-secondary" disabled={blocked || finalReview || applyUncertain} onClick={() => void run('check')}>{ja ? '復元準備の状態を確認' : 'Check restore preparation'}</button>
        {status && <button className="button button-secondary" disabled={blocked || finalReview || applyUncertain} onClick={() => void run('clear')}>{ja ? '選択を解除（安全保存は保持）' : 'Clear selection (keep safety backups)'}</button>}
      </div>
      {(loading || busy) && <p role="status">{ja ? '確認中…' : 'Checking…'}</p>}
      {(failed || status?.state === 'unavailable') && <p className="workspace-error" role="alert">{ja ? '選択内容または保存結果を確認できませんでした。状態を再確認するか、選択を解除してやり直してください。安全保存は保持されます。' : 'Could not confirm the selection or save result. Check its status or clear the selection and start again. Safety backups are preserved.'}</p>}
      {status && <p className="restore-path"><strong>{ja ? '選択したファイル' : 'Selected file'}</strong><br />{status.path}</p>}
      {preview && <>
        <dl className="restore-metadata"><dt>{ja ? '保存日時' : 'Created'}</dt><dd>{new Date(preview.manifest.created_at).toLocaleString(language)}</dd><dt>{ja ? 'データサイズ' : 'Database size'}</dt><dd>{preview.manifest.database_bytes.toLocaleString(language)} {ja ? 'バイト' : 'bytes'}</dd></dl>
        <p>{ja ? '次のRULESの内容と復元先を確認してください。ローカル復元先の移動は、この段階では指定できません。' : 'Review these RULES contents and destinations. Local destinations cannot be relocated at this stage.'}</p>
        <div className="restore-targets">{preview.targets.map((target, index) => <section className="restore-target" key={`${target.project_id ?? 'global'}-${index}`}>
          <h3>{target.project_id === null ? ja ? '全体のRULES' : 'Global RULES' : ja ? `プロジェクト ${target.project_id}` : `Project ${target.project_id}`}</h3>
          <p className="restore-path"><strong>{ja ? '復元先' : 'Destination'}</strong><br />{target.target_path}</p>
          <details><summary>{ja ? '保存されている内容を表示' : 'Show saved contents'}</summary>{preview.manifest.rules[index]?.content === null ? <p>{ja ? '保存時はRULESが未配置でした。' : 'RULES was absent when saved.'}</p> : <pre className="restore-content">{preview.manifest.rules[index]?.content}</pre>}</details>
        </section>)}</div>
        {status.state !== 'prepared' && !terminal && <>
          <label className="restore-confirmation"><input type="checkbox" checked={confirmed} disabled={blocked || failed || unavailable} onChange={e => setConfirmed(e.target.checked)} />{ja ? 'バックアップの内容とすべての復元先を確認しました。' : 'I reviewed the backup contents and every destination.'}</label>
          <button className="button button-primary" disabled={blocked || failed || !confirmed || unavailable} onClick={() => void run('prepare')}>{status.can_retry || status.preparation ? ja ? '同じ安全保存を再試行' : 'Retry the same safety save' : ja ? '現在の状態を安全保存' : 'Save current state for safety'}</button>
        </>}
      </>}
      {status?.state === 'prepared' && <p role="status">{ja ? '現在の状態を安全保存しました。復元はまだ適用していません。' : 'The current state was saved for safety. The restore has not been applied.'}</p>}
      {status?.state === 'prepared' && !finalReview && <button className="button button-secondary" disabled={blocked || failed || applyUncertain || terminal} onClick={() => { setFinalReview(true); setConfirmed(false) }}>{ja ? '復元適用の最終確認へ' : 'Review restore application'}</button>}
      {finalReview && <section className="restore-final-confirmation" role="alertdialog" aria-labelledby="restore-final-title" aria-describedby="restore-final-description">
        <h3 id="restore-final-title">{ja ? 'バックアップの内容で置き換えますか？' : 'Replace with the backup contents?'}</h3>
        <p id="restore-final-description">{ja ? '会話・設定と確認したRULES復元先を置き換えます。ユーザーとAIのすべてのログイン資格と変更カーソルが失効します。ユーザーは再ログインし、AIは新しい資格を取得する必要があります。失敗時の巻き戻し後も再認証が必要です。' : 'Conversations, settings and the reviewed RULES destinations will be replaced. All human and AI login credentials and change cursors will expire. Humans must sign in again and AI must obtain new credentials. Reauthentication is also required after rollback.'}</p>
        <label className="restore-confirmation"><input autoFocus type="checkbox" checked={confirmed} disabled={blocked} onChange={e => setConfirmed(e.target.checked)} />{ja ? '置き換え対象と資格失効を理解し、復元を適用します。' : 'I understand the replacement and credential expiry and want to apply this restore.'}</label>
        <div className="notification-settings-actions"><button className="button button-secondary" disabled={blocked} onClick={() => { setFinalReview(false); setConfirmed(false) }}>{ja ? '戻る' : 'Go back'}</button><button className="button button-primary" disabled={blocked || !confirmed} onClick={() => void apply()}>{ja ? '復元を適用して再ログイン' : 'Apply restore and sign in again'}</button></div>
      </section>}
      {(outcome || status?.state === 'applied' || status?.state === 'rolled_back') && <p role={applyUncertain ? 'alert' : 'status'} className={applyUncertain ? 'workspace-error' : undefined}>{applyUncertain ? ja ? '適用結果が不確定です。自動再送はしません。再ログイン後に状態を確認してください。復旧が必要な場合はruntimeを再起動してください。' : 'Application result is uncertain. It will not be resent automatically. Sign in again to check its status. Restart the runtime if recovery is required.' : (outcome || status?.state) === 'applied' ? ja ? '復元を適用しました。再ログインしてください。' : 'Restore applied. Sign in again.' : ja ? '復元前の状態へ巻き戻しました。再ログインしてください。' : 'Rolled back to the state before application. Sign in again.'}</p>}
      {status?.preparation?.safety_key && <p className="restore-path"><strong>{ja ? '安全保存のファイル' : 'Safety backup file'}</strong><br />{status.preparation.safety_directory}/{status.preparation.safety_key}.backup</p>}
    </div>
  </details>
}
