import { useEffect, useRef, useState } from 'react'
import { guiRulesManage } from './bridge'
import type { RulesRecoverySnapshot } from './types'

const copy = {
  en: {
    title: 'Recover Rules', id: 'Recovery ID', inspect: 'Compare recovery copy', apply: 'Apply this exact recovery copy',
    guide: 'Enter the Recovery ID reported by an uncertain save. Compare the copy with the current document and your draft before applying it. Recovery copies are retained. A copy may contain the previous document or the intended source of a first creation.',
    draft: 'Your current draft (preserved)', original: 'Document originally loaded in this editor', notLoaded: 'No original document was loaded in this editor.', current: 'Current file', candidate: 'Recovery copy',
    missing: 'The current file is missing.', utf8: 'The current file is not valid UTF-8. Its exact byte revision is shown; no replacement text is substituted.',
    confirm: 'Replace the current file with this copy? The displayed current revision and recovery copy must still match. Your draft will remain available.',
    cancel: 'Cancel', review: 'Review recovery', failed: 'Recovery failed or a compared file changed. Your draft is retained. Compare again before retrying.',
    success: 'The recovery copy was applied and read back. Your draft is retained.', invalid: 'This copy is not a valid Rules document and cannot be applied.',
  },
  ja: {
    title: 'ルールの復旧', id: '復旧ID', inspect: '退避コピーと比較', apply: 'この退避コピーを適用',
    guide: '保存結果が不確定なときに表示された復旧IDを入力してください。現在の内容・下書きと比較してから適用します。退避コピーは保持します。コピーには変更前の内容、または初回作成時の予定内容が入っています。',
    draft: '現在の下書き（保持）', original: '編集開始時に読み込んだ内容', notLoaded: 'この編集画面では元の文書を読み込めていません。', current: '現在のファイル', candidate: '退避コピー',
    missing: '現在のファイルはありません。', utf8: '現在のファイルは正しいUTF-8ではありません。実際のバイト列の版を表示し、代わりの文章へ置き換えません。',
    confirm: '現在のファイルをこのコピーで置き換えますか？比較した現在の版と退避コピーが変わっている場合は拒否します。下書きは保持します。',
    cancel: 'キャンセル', review: '復旧内容を確認', failed: '復旧に失敗したか、比較したファイルが変わりました。下書きは保持しています。もう一度比較してください。',
    success: '退避コピーを適用し、読み戻して確認しました。下書きは保持しています。', invalid: 'このコピーは正しいルール文書ではないため適用できません。',
  },
}

export default function RulesRecoveryPanel({ language, accountId, scope, draft, original, canSave, blocked, beginOperation, endOperation, onRecovered }: {
  language: 'ja' | 'en'; accountId: string; scope: string; draft: string; original: string | null; canSave: boolean; blocked: boolean
  beginOperation: () => boolean; endOperation: () => void; onRecovered: () => Promise<void>
}) {
  const t = copy[language]
  const [id, setId] = useState('')
  const [snapshot, setSnapshot] = useState<RulesRecoverySnapshot | null>(null)
  const [review, setReview] = useState(false)
  const [working, setWorking] = useState(false)
  const [status, setStatus] = useState<'failed' | 'success' | null>(null)
  const generation = useRef(0)
  const running = useRef(false)
  useEffect(() => {
    generation.current++
    setId(''); setSnapshot(null); setReview(false); setStatus(null); setWorking(false)
    return () => { generation.current++ }
  }, [accountId, scope])
  const scopeArgs = scope === 'global' ? {} : { project_id: scope }
  const disabled = blocked || working
  async function perform(apply: boolean) {
    if (disabled || running.current || !/^[0-9a-f]{32}$/.test(id) || (apply && (!snapshot?.candidate.valid || !canSave)) || !beginOperation()) return
    running.current = true
    const request = ++generation.current
    setWorking(true); setStatus(null); setReview(false)
    try {
      if (apply && snapshot) {
        const result = await guiRulesManage('rules.recovery.apply', {
          ...scopeArgs, recovery_id: snapshot.recovery_id, recovery_revision: snapshot.candidate.revision,
          source: snapshot.candidate.source, if_revision: snapshot.current.revision,
        })
        if (request !== generation.current) return
        if (result.source !== snapshot.candidate.source || result.revision !== snapshot.candidate.revision) throw new Error('Recovery verification failed')
        setSnapshot(null)
        await onRecovered()
        if (request === generation.current) setStatus('success')
      } else {
        setSnapshot(null)
        const result = await guiRulesManage('rules.recovery.inspect', { ...scopeArgs, recovery_id: id })
        if (request !== generation.current) return
        if (result.recovery_id !== id || result.project_id !== (scope === 'global' ? null : scope) || result.scope !== (scope === 'global' ? 'global' : 'project')) throw new Error('Mismatched recovery scope')
        setSnapshot(result)
      }
    } catch {
      if (request === generation.current) { setSnapshot(null); setStatus('failed') }
    } finally {
      running.current = false; endOperation()
      if (request === generation.current) setWorking(false)
    }
  }
  function sourcePreview(label: string, source: string, revision?: string) {
    return <details className="rules-recovery-source"><summary>{label}</summary>{revision && <code>{revision}</code>}<pre>{source}</pre></details>
  }
  return <details className="rules-recovery-panel"><summary>{t.title}</summary>
    <p className="rules-guide">{t.guide}</p>
    <label className="rules-field"><span>{t.id}</span><input data-testid="rules-recovery-id" value={id} maxLength={32} autoComplete="off" spellCheck={false} disabled={disabled} onChange={(event) => { generation.current++; setId(event.target.value.trim()); setSnapshot(null); setReview(false); setStatus(null) }} /></label>
    <div className="rules-actions"><button className="button button-secondary" type="button" data-testid="rules-recovery-inspect" disabled={disabled || !/^[0-9a-f]{32}$/.test(id)} onClick={() => void perform(false)}>{t.inspect}</button></div>
    {status && <p role={status === 'failed' ? 'alert' : 'status'}>{t[status]}</p>}
    {snapshot && <div className="rules-recovery-comparison" data-testid="rules-recovery-comparison">
      {original === null ? <p>{t.notLoaded}</p> : sourcePreview(t.original, original)}{sourcePreview(t.draft, draft)}
      {sourcePreview(t.current, snapshot.current.source ?? '', snapshot.current.revision)}
      {!snapshot.current.exists && <p>{t.missing}</p>}{snapshot.current.exists && !snapshot.current.utf8 && <p>{t.utf8}</p>}
      {sourcePreview(t.candidate, snapshot.candidate.source, snapshot.candidate.revision)}
      {!snapshot.candidate.valid && <p role="alert">{t.invalid}</p>}
      <div className="rules-actions"><button className="button button-secondary" type="button" data-testid="rules-recovery-review" disabled={disabled || !canSave || !snapshot.candidate.valid} onClick={() => setReview(true)}>{t.review}</button></div>
      {review && <div className="rules-inline-confirm" role="alertdialog" aria-labelledby="rules-recovery-confirm-title">
        <h4 id="rules-recovery-confirm-title">{t.review}</h4><p>{t.confirm}</p>
        <div className="rules-actions"><button className="button button-primary" type="button" data-testid="rules-recovery-apply" disabled={disabled || !canSave} onClick={() => void perform(true)}>{t.apply}</button><button className="button button-secondary" type="button" disabled={disabled} onClick={() => setReview(false)}>{t.cancel}</button></div>
      </div>}
    </div>}
  </details>
}
