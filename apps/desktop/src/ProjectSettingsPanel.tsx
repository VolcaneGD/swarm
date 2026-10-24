import { useCallback, useEffect, useRef, useState, type FormEvent } from 'react'
import { guiProjectList, guiProjectRegister, type Project } from './bridge'
import { ProjectUsageGuide } from './SwarmUsageGuides'

const copy = {
  ja: {
    title: 'プロジェクト登録', guide: 'AI sessionを開始する前に、作業するプロジェクトを登録してください。登録はAIへの書き込み許可を追加しません。',
    name: 'プロジェクト名', path: '正本フォルダー', pathHint: '既存フォルダーの絶対パスを入力してください。共通ルールは常に適用され、このフォルダーのRULES.mdは必要な場合だけ追加できます。',
    register: '登録', working: '登録中…', refresh: '一覧を更新', loading: '読み込み中…', empty: '登録済みプロジェクトはありません。',
    failed: '結果を確認できません。一覧を更新し、登録状況を確認してから再操作してください。', invalid: 'プロジェクト名と既存フォルダーのパスを入力してください。',
    saved: '登録を確認しました。同じフォルダーが登録済みの場合は、既存の登録を表示します。', id: 'project ID（CLIの --project-id に指定）',
    rules: 'AIの操作許可は「権限・ルール」で設定します。別パスの関連付け・正本変更はこのフォームでは行いません。',
  },
  en: {
    title: 'Project registration', guide: 'Register a working project before starting an AI session. Registration does not grant AI write permissions.',
    name: 'Project name', path: 'Canonical folder', pathHint: 'Enter the absolute path of an existing folder. Global Rules always apply; add RULES.md in this folder only when needed.',
    register: 'Register', working: 'Registering…', refresh: 'Refresh list', loading: 'Loading…', empty: 'No registered projects yet.',
    failed: 'The result could not be confirmed. Refresh the list and check registration before another action.', invalid: 'Enter a project name and an existing folder path.',
    saved: 'Registration confirmed. If the same folder is already registered, its existing registration is shown.', id: 'Project ID (CLI --project-id)',
    rules: 'Configure AI permissions in Permissions & rules. This form does not associate other paths or change the canonical root.',
  },
}

export default function ProjectSettingsPanel({ accountId, language, disabled, beginOperation, endOperation }: {
  accountId: string; language: 'ja' | 'en'; disabled: boolean; beginOperation: () => boolean; endOperation: () => void
}) {
  const t = copy[language]
  const [projects, setProjects] = useState<Project[]>([])
  const [name, setName] = useState('')
  const [path, setPath] = useState('')
  const [loading, setLoading] = useState(true)
  const [working, setWorking] = useState(false)
  const [ready, setReady] = useState(false)
  const [error, setError] = useState('')
  const [notice, setNotice] = useState(false)
  const generation = useRef(0)
  const running = useRef(false)
  const refresh = useCallback(async () => {
    const current = ++generation.current
    setLoading(true)
    setReady(false)
    setProjects([])
    try {
      const result = await guiProjectList(accountId)
      if (current !== generation.current) return
      setProjects(result.items)
      setReady(true)
      setError('')
    } catch {
      if (current === generation.current) setError('failed')
    } finally {
      if (current === generation.current) setLoading(false)
    }
  }, [accountId])
  useEffect(() => { void refresh(); return () => { generation.current++ } }, [refresh])

  async function register(event: FormEvent) {
    event.preventDefault()
    if (disabled || loading || !ready || running.current) return
    if (!name.trim() || !path.trim()) { setError('invalid'); return }
    if (!beginOperation()) return
    const current = generation.current
    running.current = true
    setWorking(true)
    setError('')
    setNotice(false)
    try {
      const project = await guiProjectRegister(accountId, name.trim(), path.trim())
      if (current !== generation.current) return
      // Keep a confirmed result even if a later list refresh is unavailable.
      setProjects((prior) => [...prior.filter((item) => item.id !== project.id), project])
      setName('')
      setPath('')
      setNotice(true)
    } catch {
      if (current === generation.current) { setError('failed'); setReady(false); setProjects([]) }
    } finally {
      running.current = false
      if (current === generation.current) setWorking(false)
      endOperation()
    }
  }
  const locked = disabled || working || loading
  return <section className="project-settings" aria-labelledby="project-settings-title" data-testid="project-settings">
    <h3 id="project-settings-title">{t.title}</h3>
    <p>{t.guide}</p>
    <ProjectUsageGuide language={language} />
    <form className="project-registration-form" onSubmit={(event) => void register(event)}>
      <label>{t.name}<input data-testid="project-name" value={name} onChange={(event) => setName(event.target.value)} disabled={locked || !ready} maxLength={512} required /></label>
      <label>{t.path}<input data-testid="project-root" value={path} onChange={(event) => setPath(event.target.value)} disabled={locked || !ready} maxLength={32768} required aria-describedby="project-path-hint" /><small id="project-path-hint">{t.pathHint}</small></label>
      <div className="project-actions">
        <button className="button button-primary" type="submit" disabled={locked || !ready} data-testid="project-register">{working ? t.working : t.register}</button>
        <button className="button button-secondary" type="button" onClick={() => void refresh()} disabled={locked} data-testid="project-refresh">{t.refresh}</button>
      </div>
    </form>
    {error && <p className="runtime-error" role="alert">{error === 'invalid' ? t.invalid : t.failed}</p>}
    {notice && <p role="status">{t.saved}</p>}
    {loading ? <p role="status">{t.loading}</p> : <div className="project-list">
      {ready && projects.length === 0 && <p>{t.empty}</p>}
      {projects.map((project) => <article className="project-card" key={project.id}>
        <h4>{project.name}</h4><p className="project-path">{project.canonical_root}</p>
        <label>{t.id}<input readOnly value={project.id} onFocus={(event) => event.currentTarget.select()} /></label>
      </article>)}
    </div>}
    <p>{t.rules}</p>
  </section>
}
