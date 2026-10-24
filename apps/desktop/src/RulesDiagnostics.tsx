import { useEffect, useMemo, useRef, useState } from 'react'
import { guiRead, guiRulesCheck } from './bridge'
import type { RulesCapabilities, RulesCheckResult, RulesCheckTarget, RulesSubject } from './types'

type Language = 'en' | 'ja'
type TargetField = keyof RulesCheckTarget
type TargetSpec = { fields: TargetField[]; optional?: TargetField[] }

const copy = {
  en: {
    title: 'Saved Rules permission check', explanation: 'Checks saved global and project Rules together with fixed AI restrictions for a registered AI in the selected project. Unsaved edits are not checked. An allowed result covers permission only; it does not validate the request body or reserve a later operation.',
    denied: 'Permission checking is not available for this Rules scope.', noProjects: 'No readable project with permission to check Rules is available.', project: 'Project', actor: 'Registered AI actor', chooseActor: 'Choose an AI actor', loadingActors: 'Loading registered AI actors…', noActors: 'No registered AI actors are available for this project.', actorError: 'Could not load registered AI actors. Refresh and try again.', refreshActors: 'Refresh actors',
    operation: 'Operation', chooseOperation: 'Choose an operation', target: 'Target', context: 'Context (JSON object)', contextHint: 'Optional caller context. Enter a JSON object no larger than 64 KiB in UTF-8.',
    targetId: 'Target ID', boardId: 'Board ID', threadId: 'Thread ID', postNo: 'Post No.', replyTo: 'Reply target No.', searchBoard: 'Board ID filter (optional)', searchThread: 'Thread ID filter (optional)',
    run: 'Check saved Rules', checking: 'Checking…', result: 'Permission result', allowed: 'Allowed by current effective permissions', deniedResult: 'Denied by current effective permissions', reasons: {
      allowed: 'Configured Rules and fixed AI restrictions allow this operation.', not_allowed: 'The effective Rules do not allow this operation.', operation_prohibited: 'This operation is prohibited.', target_unavailable: 'The target is unavailable under current access rules.', owner_only: 'This operation is limited to the owner.', thread_full: 'The Thread has reached its Post limit.', not_supported: 'This operation is not supported by the permission checker.',
    }, unknownReason: 'The operation was denied or could not be evaluated.', checkError: 'The permission check could not be completed. Refresh the inputs and try again.', invalidContext: 'Context must be a JSON object no larger than 64 KiB in UTF-8.', invalidTarget: 'Enter a valid positive whole number for the target number.', stale: 'The inputs changed; run the check again.',
  },
  ja: {
    title: '保存済みルールの権限確認', explanation: '選択したprojectで実登録AIに対する全体・projectの保存済みルールとAIの固定制約を合わせて確認します。未保存の編集内容は対象外です。「許可」は本文の妥当性を保証せず、後続操作を予約しません。',
    denied: 'このルール範囲では権限確認を利用できません。', noProjects: 'ルール確認の権限がある閲覧可能なprojectがありません。', project: 'project', actor: '登録済みAI', chooseActor: 'AIを選択', loadingActors: '登録済みAIを読み込み中…', noActors: 'このprojectに登録済みAIはありません。', actorError: '登録済みAIを読み込めません。更新して再試行してください。', refreshActors: 'AI一覧を更新',
    operation: '操作', chooseOperation: '操作を選択', target: '対象', context: 'コンテキスト（JSONオブジェクト）', contextHint: '呼び出し元のコンテキスト（任意）。UTF-8で64 KiB以下のJSONオブジェクトを入力してください。',
    targetId: '対象ID', boardId: 'ボードID', threadId: 'スレッドID', postNo: '投稿No.', replyTo: '返信先No.', searchBoard: 'ボードIDで絞り込み（任意）', searchThread: 'スレッドIDで絞り込み（任意）',
    run: '保存済みルールを確認', checking: '確認中…', result: '権限確認結果', allowed: '現在の実効権限で許可', deniedResult: '現在の実効権限で拒否', reasons: {
      allowed: '設定されたルールとAIの固定制約により、この操作は許可されています。', not_allowed: '実効ルールでこの操作は許可されていません。', operation_prohibited: 'この操作は禁止されています。', target_unavailable: '現在のアクセスルールでは対象を利用できません。', owner_only: 'この操作は作成者本人に限られています。', thread_full: 'スレッドの投稿上限に達しています。', not_supported: 'この操作は権限確認に対応していません。',
    }, unknownReason: '操作は拒否されたか、評価できませんでした。', checkError: '権限を確認できません。入力を更新して再試行してください。', invalidContext: 'コンテキストはUTF-8で64 KiB以下のJSONオブジェクトにしてください。', invalidTarget: '対象番号は1以上の整数を入力してください。', stale: '入力が変更されました。もう一度確認してください。',
  },
} as const

const emptyTargetSpecs: TargetSpec = { fields: [] }

function targetSpec(operation: string): TargetSpec {
  switch (operation) {
    case 'board.get': case 'board.edit': case 'project.get':
    case 'thread.get': case 'thread.edit':
      return { fields: ['id'] }
    case 'thread.create': case 'thread.list':
      return { fields: ['board_id'], optional: operation === 'thread.list' ? ['board_id'] : [] }
    case 'post.list': case 'post.create':
      return { fields: ['thread_id'] }
    case 'post.get': case 'post.edit': case 'post.history':
      return { fields: ['thread_id', 'no'] }
    case 'response.create': case 'response.list':
      return { fields: ['thread_id', 'to'] }
    case 'search':
      return { fields: [], optional: ['board_id', 'thread_id'] }
    default:
      return emptyTargetSpecs
  }
}

function fieldLabel(field: TargetField, language: Language): string {
  const t = copy[language]
  switch (field) {
    case 'id': return t.targetId
    case 'board_id': return t.boardId
    case 'thread_id': return t.threadId
    case 'no': return t.postNo
    case 'to': return t.replyTo
  }
}

function parseContext(text: string): Record<string, unknown> | null {
  if (new TextEncoder().encode(text).length > 64 * 1024) return null
  try {
    const value: unknown = JSON.parse(text)
    if (!value || typeof value !== 'object' || Array.isArray(value)) return null
    return value as Record<string, unknown>
  } catch {
    return null
  }
}

function localizedReason(reason: string, language: Language): string {
  const t = copy[language]
  return (t.reasons as Record<string, string>)[reason] ?? t.unknownReason
}

export default function RulesDiagnostics({
  capabilities,
  currentAccountId,
  busy = false,
  language,
  rulesProjectId,
  beginOperation,
  endOperation,
}: {
  capabilities: RulesCapabilities
  currentAccountId: string
  busy?: boolean
  language: Language
  rulesProjectId: string | null
  beginOperation: () => boolean
  endOperation: () => void
}) {
  const t = copy[language]
  const candidates = useMemo(() => capabilities.projects.filter((project) => project.read && project.check), [capabilities.projects])
  const allowedByScope = rulesProjectId === null
    ? Boolean(capabilities.global.check)
    : Boolean(capabilities.projects.find((project) => project.id === rulesProjectId && project.read && project.check))
  const projectOptions = useMemo(() => rulesProjectId === null ? candidates : candidates.filter((project) => project.id === rulesProjectId), [candidates, rulesProjectId])
  const [open, setOpen] = useState(false)
  const [projectId, setProjectId] = useState(projectOptions[0]?.id ?? '')
  const [subjects, setSubjects] = useState<RulesSubject[]>([])
  const [subjectsState, setSubjectsState] = useState<'idle' | 'loading' | 'ready' | 'failed'>('idle')
  const [actorId, setActorId] = useState('')
  const [operation, setOperation] = useState('')
  const [targetValues, setTargetValues] = useState<Partial<Record<TargetField, string>>>({})
  const [contextText, setContextText] = useState('{}')
  const [contextError, setContextError] = useState('')
  const [targetError, setTargetError] = useState('')
  const [checkState, setCheckState] = useState<'idle' | 'checking' | 'complete' | 'failed'>('idle')
  const [result, setResult] = useState<RulesCheckResult | null>(null)
  const loadGeneration = useRef(0)
  const checkGeneration = useRef(0)
  const currentInputKey = useRef('')
  const capabilitySignature = JSON.stringify({
    global: capabilities.global,
    projects: capabilities.projects.map(({ id, read, check }) => ({ id, read, check })),
    actions: capabilities.actions,
  })
  currentInputKey.current = JSON.stringify({ currentAccountId, rulesProjectId, projectId, actorId, operation, targetValues, contextText, capabilitySignature })
  const spec = targetSpec(operation)
  const targetFields = Array.from(new Set([...spec.fields, ...(spec.optional ?? [])]))
  const context = parseContext(contextText)
  const subjectsReady = subjectsState === 'ready' && subjects.length > 0
  const requiredTargetMissing = spec.fields.some((field) => !spec.optional?.includes(field) && !String(targetValues[field] ?? '').trim())
  const runDisabled = busy || !allowedByScope || !projectId || !actorId || !operation || !subjectsReady || checkState === 'checking' || !context || requiredTargetMissing

  const invalidateCheck = () => {
    checkGeneration.current += 1
    setResult(null)
    setCheckState('idle')
  }

  useEffect(() => {
    const load = ++loadGeneration.current
    checkGeneration.current += 1
    setResult(null)
    setCheckState('idle')
    setActorId('')
    setSubjects([])
    if (!open || !allowedByScope || !projectId || !projectOptions.some((project) => project.id === projectId)) {
      if (allowedByScope && projectOptions.length > 0 && !projectOptions.some((project) => project.id === projectId)) setProjectId(projectOptions[0].id)
      setSubjectsState('idle')
      return () => { loadGeneration.current += 1 }
    }
    setSubjectsState('loading')
    void guiRead('rules.subjects', { project_id: projectId }).then(({ items }) => {
      if (load !== loadGeneration.current) return
      setSubjects(items.filter((item) => item.project_id === projectId))
      setActorId(items.find((item) => item.project_id === projectId)?.id ?? '')
      setSubjectsState('ready')
    }).catch(() => {
      if (load !== loadGeneration.current) return
      setSubjectsState('failed')
    })
    return () => { if (load === loadGeneration.current) loadGeneration.current += 1 }
  }, [currentAccountId, open, projectId, projectOptions, allowedByScope])

  useEffect(() => {
    checkGeneration.current += 1
    setResult(null)
    setCheckState('idle')
    setContextError('')
    setTargetError('')
    if (!operation || !capabilities.actions.includes(operation)) setOperation(capabilities.actions[0] ?? '')
  }, [capabilitySignature, capabilities.actions, projectId, rulesProjectId])

  useEffect(() => {
    if (!allowedByScope) {
      checkGeneration.current += 1
      setResult(null)
      setCheckState('idle')
      setSubjects([])
      setActorId('')
      setSubjectsState('idle')
    }
  }, [allowedByScope])

  const refreshSubjects = () => {
    setSubjectsState('idle')
    setSubjects([])
    setActorId('')
    invalidateCheck()
    // Trigger the same guarded lookup without a background polling loop.
    const load = ++loadGeneration.current
    if (!allowedByScope || !projectId || !projectOptions.some((project) => project.id === projectId)) return
    setSubjectsState('loading')
    void guiRead('rules.subjects', { project_id: projectId }).then(({ items }) => {
      if (load !== loadGeneration.current) return
      const safeItems = items.filter((item) => item.project_id === projectId)
      setSubjects(safeItems)
      setActorId(safeItems[0]?.id ?? '')
      setSubjectsState('ready')
    }).catch(() => {
      if (load !== loadGeneration.current) return
      setSubjectsState('failed')
    })
  }

  const runCheck = async () => {
    if (runDisabled) return
    const checkedContext = parseContext(contextText)
    if (!checkedContext) {
      setContextError(t.invalidContext)
      invalidateCheck()
      return
    }
    const target: RulesCheckTarget = {}
    for (const field of targetFields) {
      const raw = String(targetValues[field] ?? '').trim()
      if (!raw && spec.optional?.includes(field)) continue
      if (field === 'no' || field === 'to') {
        const number = Number(raw)
        if (!Number.isSafeInteger(number) || number <= 0) {
          setTargetError(t.invalidTarget)
          invalidateCheck()
          return
        }
        target[field] = number
      } else if (raw) target[field] = raw
    }
    setContextError('')
    setTargetError('')
    if (!beginOperation()) return
    const request = ++checkGeneration.current
    const inputKey = currentInputKey.current
    setResult(null)
    setCheckState('checking')
    try {
      const checked = await guiRulesCheck({ project_id: projectId, actor_id: actorId, operation, ...(Object.keys(target).length ? { target } : {}) }, checkedContext)
      if (request !== checkGeneration.current || inputKey !== currentInputKey.current) return
      setResult(checked)
      setCheckState('complete')
    } catch {
      if (request !== checkGeneration.current || inputKey !== currentInputKey.current) return
      setCheckState('failed')
    } finally {
      endOperation()
    }
  }

  const setTarget = (field: TargetField, value: string) => {
    setTargetValues((current) => ({ ...current, [field]: value }))
    setTargetError('')
    invalidateCheck()
  }

  return <details className="rules-diagnostics-disclosure" open={open} onToggle={(event) => setOpen(event.currentTarget.open)} data-testid="rules-diagnostics-disclosure">
    <summary>{t.title}<span aria-hidden="true">{open ? '−' : '+'}</span></summary>
    <section className="rules-diagnostics" aria-label={t.title}>
      <p className="rules-diagnostics-explanation">{t.explanation}</p>
      {!allowedByScope && <p className="rules-diagnostics-message" role="status" data-testid="rules-check-denied">{t.denied}</p>}
      {allowedByScope && !projectOptions.length && <p className="rules-diagnostics-message" role="status" data-testid="rules-check-no-projects">{t.noProjects}</p>}
      {allowedByScope && projectOptions.length > 0 && <>
        <div className="rules-diagnostics-form">
          <label className="rules-diagnostics-field"><span>{t.project}</span><select data-testid="rules-check-project" value={projectId} onChange={(event) => { setProjectId(event.target.value); invalidateCheck() }} disabled={busy || projectOptions.length < 2}>
            {projectOptions.map((project) => <option key={project.id} value={project.id}>{project.name}</option>)}
          </select></label>
          <label className="rules-diagnostics-field"><span>{t.actor}</span><select data-testid="rules-check-actor" value={actorId} onChange={(event) => { setActorId(event.target.value); invalidateCheck() }} disabled={busy || subjectsState !== 'ready' || !subjects.length}>
            <option value="">{t.chooseActor}</option>{subjects.map((subject) => <option key={subject.id} value={subject.id}>{subject.model_name}: {subject.id}</option>)}
          </select></label>
          {subjectsState === 'loading' && <p className="rules-diagnostics-message" role="status" data-testid="rules-subjects-loading">{t.loadingActors}</p>}
          {subjectsState === 'failed' && <div className="rules-diagnostics-error" role="alert"><p>{t.actorError}</p><button className="button button-secondary" type="button" onClick={refreshSubjects} disabled={busy}>{t.refreshActors}</button></div>}
          {subjectsState === 'ready' && !subjects.length && <p className="rules-diagnostics-message" role="status" data-testid="rules-subjects-empty">{t.noActors}</p>}
          <label className="rules-diagnostics-field"><span>{t.operation}</span><select data-testid="rules-check-operation" value={operation} onChange={(event) => { setOperation(event.target.value); setTargetValues({}); invalidateCheck() }} disabled={busy || !capabilities.actions.length}>
            <option value="">{t.chooseOperation}</option>{capabilities.actions.map((action) => <option key={action} value={action}>{action}</option>)}
          </select></label>
          {targetFields.length > 0 && <div className="rules-diagnostics-targets" data-testid="rules-check-targets"><strong>{t.target}</strong>{targetFields.map((field) => <label className="rules-diagnostics-field" key={field}><span>{fieldLabel(field, language)}{spec.optional?.includes(field) ? '' : ' *'}</span><input data-testid={`rules-check-target-${field}`} type={field === 'no' || field === 'to' ? 'number' : 'text'} min={field === 'no' || field === 'to' ? 1 : undefined} step={field === 'no' || field === 'to' ? 1 : undefined} value={targetValues[field] ?? ''} onChange={(event) => setTarget(field, event.target.value)} disabled={busy} /></label>)}</div>}
          <label className="rules-diagnostics-field"><span>{t.context}</span><textarea data-testid="rules-check-context" value={contextText} onChange={(event) => { setContextText(event.target.value); setContextError(''); invalidateCheck() }} disabled={busy} aria-describedby="rules-check-context-hint" /></label>
          <small id="rules-check-context-hint" className="rules-diagnostics-hint">{t.contextHint}</small>
          {(!!contextError || !context) && <p className="rules-diagnostics-error-text" role="alert" data-testid="rules-check-context-error">{contextError || t.invalidContext}</p>}
          {!!targetError && <p className="rules-diagnostics-error-text" role="alert" data-testid="rules-check-target-error">{targetError}</p>}
          {checkState === 'failed' && <p className="rules-diagnostics-error-text" role="alert">{t.checkError}</p>}
          <button className="button button-secondary" data-testid="rules-check-run" type="button" onClick={() => void runCheck()} disabled={runDisabled}>{checkState === 'checking' ? t.checking : t.run}</button>
        </div>
        {checkState === 'complete' && result && <section className={`rules-check-result ${result.allowed ? 'is-allowed' : 'is-denied'}`} data-testid="rules-check-result" aria-label={t.result} aria-live="polite">
          <strong>{result.allowed ? t.allowed : t.deniedResult}</strong><p>{localizedReason(result.reason, language)}</p>
        </section>}
      </>}
    </section>
  </details>
}
