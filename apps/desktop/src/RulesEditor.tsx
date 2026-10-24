import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent, type KeyboardEvent } from 'react'
import useSettingsCapabilityGuard from './useSettingsCapabilityGuard'
import { guiCreationStatus, guiRead, guiRulesManage } from './bridge'
import type { CreationStatus, RulesCapabilities, RulesDocument, RulesValidation } from './types'
import { appendRuleDraft, DEFAULT_RULES_TEMPLATE, type RuleDraft } from './rulesDraft'
import RulesDiagnostics from './RulesDiagnostics'
import RulesRecoveryPanel from './RulesRecoveryPanel'

type Language = 'en' | 'ja'
type ScopeKey = 'global' | string
type LoadState = 'loading' | 'ready' | 'failed'
type DraftRecord = { source: string; revision: string; originalSource: string }
type ValidationRecord = RulesValidation & { source: string; scopeKey: ScopeKey }
type ConditionOperator = NonNullable<RuleDraft['condition']>['op']

const MAX_RULES_BYTES = 1024 * 1024

const copy = {
  en: {
    title: 'Rules editor', loading: 'Loading Rules permissions…', retry: 'Refresh Rules permissions', failed: 'Rules permissions could not be loaded. Editing is blocked until refresh succeeds.', denied: 'This account cannot read Rules for the selected scope.',
    scope: 'Rules scope', global: 'Global Rules', project: 'Project Rules',
    tabs: 'Rules editor mode', text: 'Text', form: 'Form', source: 'RULES.md source', size: (count: number) => `${count.toLocaleString()} / 1,048,576 UTF-8 bytes`, tooLarge: 'The Rules source exceeds 1 MiB.',
    template: 'Insert example template', templateConfirmTitle: 'Replace Rules source?', templateConfirm: 'This replaces the current source with the example template. This cannot be undone.', replace: 'Replace source', cancel: 'Cancel',
    formGuide: 'The form appends one rule without rewriting existing text. Advanced conditions and other content remain in the source; use Text mode to edit them. Rules combine across global and project documents, including conditions and denials.', id: 'Rule ID', effect: 'Effect', allow: 'Allow', deny: 'Deny', operation: 'Operation', chooseOperation: 'Choose an operation', projectId: 'Subject project ID (optional)', projectIdHint: 'Leave empty to apply without a project subject. Project-local Rules always use their registered project.', targetScope: 'Target scope', noTarget: 'Global scope', board: 'Board', thread: 'Thread', targetId: 'Board or Thread ID', conditionField: 'Condition field (optional)', conditionOperator: 'Condition operator', noCondition: 'No condition', conditionJson: 'Condition value (JSON)', addRule: 'Append rule',
    validate: 'Validate draft', validating: 'Validating…', validationTitle: 'Validation result', valid: 'Valid', invalid: 'Invalid', validationContext: (scope: string) => `Scope: ${scope}`, validatedSource: 'The result applies to this exact draft.', sourceSnapshot: 'Show exact validated source', remoteSource: 'Show latest saved source', startWithTemplate: 'Insert the example template in Text mode to start a new Rules document.', actions: 'Configured actions', noActions: 'No configured actions were reported.',
    save: 'Review save', saving: 'Saving…', saveTitle: 'Review Rules change', saveImpact: 'A valid Rules change takes effect for the next AI request and applies to pending reads. Completed operations are not rolled back.', saveScope: (scope: string) => `Save changes to ${scope}?`, confirmSave: 'Save exact validated draft', saveSuccess: 'Rules saved.', saveUncertain: 'The save result is uncertain. Your draft is retained; verify the current document before deciding whether to save again.', verify: 'Verify current document', verifiedSame: 'The current document matches this draft. The saved revision was refreshed.', verifiedDifferent: 'The current document differs from this draft. Your draft remains available; review before replacing it.',
    pending: 'A conversation creation request is pending. Resolve it before validating or saving Rules.', pendingCheckFailed: 'Could not check pending creation. Refresh before validating or saving.', unavailable: 'The selected scope no longer grants the requested Rules operation. Refresh permissions.',
    conflict: 'This Rules document changed elsewhere. Your draft is retained. Reloading will discard it.', reload: 'Review latest version', reloadTitle: 'Discard this draft and reload?', reloadBody: 'The latest saved version will replace your unsaved draft.', reloadConfirm: 'Discard draft and reload',
    error: 'The Rules operation failed. Your draft is retained.', fileBusy: 'The Rules file is open or otherwise busy. Your draft and validation are retained; close the other editor, then retry Save manually.', savedButRefreshFailed: 'Rules were saved, but workspace permissions could not be refreshed. Refresh the workspace before continuing.', recoveryReference: 'Service details:',
    fieldRequired: 'Enter a rule ID and choose an operation.', conditionInvalid: 'Enter a valid JSON value for the condition.', projectReadOnly: 'Project-local Rules apply only to their registered project.',
  },
  ja: {
    title: 'ルール編集', loading: 'ルールの権限を読み込み中…', retry: 'ルールの権限を更新', failed: 'ルールの権限を読み込めません。更新が成功するまで編集できません。', denied: '選択した範囲のルールを読み取る権限がありません。',
    scope: 'ルールの範囲', global: '全体ルール', project: 'プロジェクトルール',
    tabs: 'ルール編集モード', text: 'テキスト', form: 'フォーム', source: 'RULES.md の内容', size: (count: number) => `${count.toLocaleString()} / 1,048,576 UTF-8 バイト`, tooLarge: 'ルールの内容が1 MiBを超えています。',
    template: '例のテンプレートを挿入', templateConfirmTitle: 'ルールの内容を置き換えますか？', templateConfirm: '現在の内容を例のテンプレートで置き換えます。この操作は元に戻せません。', replace: '内容を置き換える', cancel: 'キャンセル',
    formGuide: 'フォームは既存テキストを書き換えず、ルールを1件追記します。高度な条件やその他の内容は保持されます。編集にはテキストモードを使ってください。全体とプロジェクトのルールは条件や拒否を含めて組み合わされます。', id: 'ルールID', effect: '効果', allow: '許可', deny: '拒否', operation: '操作', chooseOperation: '操作を選択', projectId: '対象project ID（任意）', projectIdHint: '空欄ならproject主体を指定しません。プロジェクトルールでは登録済みprojectを常に使います。', targetScope: '対象範囲', noTarget: '全体', board: 'ボード', thread: 'スレッド', targetId: 'ボードまたはスレッドID', conditionField: '条件項目（任意）', conditionOperator: '条件演算子', noCondition: '条件なし', conditionJson: '条件値（JSON）', addRule: 'ルールを追記',
    validate: '下書きを検証', validating: '検証中…', validationTitle: '検証結果', valid: '有効', invalid: '無効', validationContext: (scope: string) => `範囲: ${scope}`, validatedSource: 'この結果は現在の下書き内容に対するものです。', sourceSnapshot: '検証した内容を表示', remoteSource: '保存済みの最新版を表示', startWithTemplate: '新しいルール文書を始めるには、テキストモードで例のテンプレートを挿入してください。', actions: '設定された操作', noActions: '設定された操作は報告されていません。',
    save: '保存内容を確認', saving: '保存中…', saveTitle: 'ルール変更を確認', saveImpact: '有効なルール変更は次のAI要求から有効になり、保留中の閲覧にも適用されます。完了済みの操作は取り消されません。', saveScope: (scope: string) => `${scope}の変更を保存しますか？`, confirmSave: '検証済みの内容を保存', saveSuccess: 'ルールを保存しました。', saveUncertain: '保存結果を確認できません。下書きは保持されています。再保存するか決める前に、現在の文書を確認してください。', verify: '現在の文書を確認', verifiedSame: '現在の文書は下書きと一致しています。保存版番号を更新しました。', verifiedDifferent: '現在の文書は下書きと異なります。下書きは保持されています。置き換える前に確認してください。',
    pending: '会話の作成要求が保留中です。ルールの検証・保存前に解決してください。', pendingCheckFailed: '保留中の作成要求を確認できません。検証・保存の前に更新してください。', unavailable: '選択した範囲でルール操作を行う権限がありません。権限を更新してください。',
    conflict: 'このルール文書は別の場所で変更されました。下書きは保持されています。再読み込みすると下書きは破棄されます。', reload: '最新版を確認', reloadTitle: '下書きを破棄して再読み込みしますか？', reloadBody: '保存済みの最新版で未保存の下書きを置き換えます。', reloadConfirm: '下書きを破棄して再読み込み',
    error: 'ルール操作に失敗しました。下書きは保持されています。', fileBusy: 'RULES.mdが別の場所で開かれているか、使用中です。下書きと検証結果は保持されています。他の編集を閉じてから、保存を手動で再試行してください。', savedButRefreshFailed: 'ルールは保存されましたが、workspaceの権限を更新できません。続行前にworkspaceを更新してください。', recoveryReference: 'サービスの詳細:',
    fieldRequired: 'ルールIDを入力し、操作を選択してください。', conditionInvalid: '条件値に正しいJSONを入力してください。', projectReadOnly: 'プロジェクトルールは登録済みの対象projectにのみ適用されます。',
  },
} as const

function scopeName(key: ScopeKey, capabilities: RulesCapabilities | null, language: Language): string {
  if (key === 'global') return copy[language].global
  const project = capabilities?.projects.find((entry) => entry.id === key)
  return project ? `${copy[language].project}: ${project.name}` : copy[language].project
}

function getFault(error: unknown): { code: string; message: string } {
  if (error && typeof error === 'object') {
    const value = error as { code?: unknown; message?: unknown; error?: unknown }
    return {
      code: typeof value.code === 'string' ? value.code : '',
      message: typeof value.message === 'string' ? value.message : typeof value.error === 'string' ? value.error : '',
    }
  }
  return { code: '', message: typeof error === 'string' ? error : '' }
}

function isRevisionConflict(error: unknown): boolean {
  const fault = getFault(error)
  const normalized = `${fault.code} ${fault.message}`.toLowerCase()
  return fault.code === 'RULES_CHANGED' || normalized.includes('conflict') || normalized.includes('revision') || normalized.includes('stale')
}

export default function RulesEditor({
  language,
  currentAccountId,
  busy = false,
  beginOperation,
  endOperation,
  onRulesChanged,
  onAccessLost,
}: {
  language: Language
  currentAccountId: string
  busy?: boolean
  beginOperation: () => boolean
  endOperation: () => void
  onRulesChanged: () => Promise<void>
  onAccessLost?: () => void
}) {
  const t = copy[language]
  const [open, setOpen] = useState(false)
  const [capabilities, setCapabilities] = useState<RulesCapabilities | null>(null)
  useSettingsCapabilityGuard(currentAccountId, 'rules.capabilities', capabilities, onAccessLost)
  const [capabilityState, setCapabilityState] = useState<LoadState>('loading')
  const [creationStatus, setCreationStatus] = useState<CreationStatus | null>(null)
  const [creationState, setCreationState] = useState<LoadState>('loading')
  const [selectedScope, setSelectedScope] = useState<ScopeKey>('global')
  const [documentState, setDocumentState] = useState<LoadState>('loading')
  const [source, setSource] = useState('')
  const [revision, setRevision] = useState('')
  const [savedSource, setSavedSource] = useState('')
  const [editorMode, setEditorMode] = useState<'text' | 'form'>('text')
  const [validation, setValidation] = useState<ValidationRecord | null>(null)
  const [showSaveConfirmation, setShowSaveConfirmation] = useState(false)
  const [showTemplateConfirmation, setShowTemplateConfirmation] = useState(false)
  const [showReloadConfirmation, setShowReloadConfirmation] = useState(false)
  const [remoteDocument, setRemoteDocument] = useState<RulesDocument | null>(null)
  const [errorText, setErrorText] = useState('')
  const [recoveryReference, setRecoveryReference] = useState('')
  const [noticeText, setNoticeText] = useState('')
  const [needsRefresh, setNeedsRefresh] = useState(false)
  const [working, setWorking] = useState(false)
  const [ruleId, setRuleId] = useState('')
  const [effect, setEffect] = useState<'allow' | 'deny'>('allow')
  const [operation, setOperation] = useState('')
  const [subjectProjects, setSubjectProjects] = useState('')
  const [targetScope, setTargetScope] = useState<'' | 'boards' | 'threads'>('')
  const [targetId, setTargetId] = useState('')
  const [conditionField, setConditionField] = useState('')
  const [conditionOperator, setConditionOperator] = useState<ConditionOperator>('eq')
  const [conditionJson, setConditionJson] = useState('')
  const [formError, setFormError] = useState('')
  const languageRef = useRef(language)
  languageRef.current = language

  const generation = useRef(0)
  const operationRunning = useRef(false)
  const mounted = useRef(false)
  const sourceRef = useRef(source)
  sourceRef.current = source
  const drafts = useRef(new Map<ScopeKey, DraftRecord>())
  const capabilitiesRef = useRef(capabilities)
  capabilitiesRef.current = capabilities
  const loadCapabilitiesRef = useRef<((keepDraft: boolean) => Promise<boolean>) | null>(null)

  const allowedScopes = useMemo(() => {
    if (!capabilities) return []
    return [
      ...(capabilities.global.read ? [{ key: 'global' as const, label: t.global }] : []),
      ...capabilities.projects.filter((project) => project.read).map((project) => ({ key: project.id, label: `${t.project}: ${project.name}` })),
    ]
  }, [capabilities, t.global, t.project])
  const selectedProject = capabilities?.projects.find((project) => project.id === selectedScope)
  const permissions = selectedScope === 'global' ? capabilities?.global : selectedProject
  const canRead = Boolean(permissions?.read)
  const canValidate = Boolean(permissions?.validate)
  const canSave = Boolean(permissions?.save)
  const sourceBytes = useMemo(() => new TextEncoder().encode(source).length, [source])
  const dirty = source !== savedSource
  const pendingCreation = creationStatus?.pending === true
  const operationBlocked = busy || working || needsRefresh || creationState !== 'ready' || pendingCreation || capabilityState !== 'ready' || !canValidate
  const validationMatches = validation?.valid === true && validation.source === source && validation.scopeKey === selectedScope
  const saveBlockedReason = busy || working
    ? language === 'ja' ? '現在の処理が完了するまでお待ちください。' : 'Wait for the current operation to finish.'
    : needsRefresh ? language === 'ja' ? '現在の文書と権限を確認してから保存してください。' : 'Verify the current document and permissions before saving.'
    : !canSave ? t.unavailable
    : !dirty ? language === 'ja' ? '保存する変更はありません。' : 'There are no changes to save.'
    : sourceBytes > MAX_RULES_BYTES ? t.tooLarge
    : !validationMatches ? language === 'ja' ? '保存前に「下書きを検証」を押し、有効な検証結果を確認してください。下書きを編集した場合は再検証が必要です。' : 'Before saving, select Validate draft and confirm a valid result. Revalidate after editing the draft.'
    : ''

  const clearValidation = useCallback(() => {
    setValidation(null)
    setShowSaveConfirmation(false)
    setNoticeText('')
  }, [])

  const loadCapabilities = useCallback(async (keepDraft: boolean): Promise<boolean> => {
    const request = ++generation.current
    clearValidation()
    setCapabilityState('loading')
    setCreationState('loading')
    setErrorText('')
    setNoticeText('')
    try {
      const [nextCapabilities, nextCreation] = await Promise.all([
        guiRead('rules.capabilities'),
        guiCreationStatus(),
      ])
      if (!mounted.current || request !== generation.current) return false
      const scopes = [
        ...(nextCapabilities.global.read ? ['global'] : []),
        ...nextCapabilities.projects.filter((project) => project.read).map((project) => project.id),
      ]
      const nextScope = scopes.includes(selectedScope) ? selectedScope : scopes[0] ?? 'global'
      setCapabilities(nextCapabilities)
      capabilitiesRef.current = nextCapabilities
      setCreationStatus(nextCreation)
      setCapabilityState('ready')
      setCreationState('ready')
      if (!scopes.includes(selectedScope)) {
        drafts.current.delete(selectedScope)
        clearValidation()
        setSource('')
        setSavedSource('')
        setRevision('')
        sourceRef.current = ''
        setRemoteDocument(null)
        setDocumentState('ready')
      }
      if (nextScope !== selectedScope) {
        setSelectedScope(nextScope)
        clearValidation()
        setRemoteDocument(null)
        setDocumentState('loading')
        if (!keepDraft) {
          setSource('')
          setSavedSource('')
          setRevision('')
        }
      }
      return true
    } catch {
      if (!mounted.current || request !== generation.current) return false
      setCapabilities(null)
      setCreationStatus(null)
      setCapabilityState('failed')
      setCreationState('failed')
      setDocumentState('failed')
      setSource('')
      setSavedSource('')
      setRevision('')
      setErrorText(copy[languageRef.current].failed)
      return false
    }
  }, [clearValidation, selectedScope, t.failed])
  loadCapabilitiesRef.current = loadCapabilities

  const readDocument = useCallback(async (key: ScopeKey, request: number, preserveDraft: boolean): Promise<void> => {
    const currentCapabilities = capabilitiesRef.current
    const scopePermissions = key === 'global' ? currentCapabilities?.global : currentCapabilities?.projects.find((project) => project.id === key)
    if (!scopePermissions?.read) {
      if (mounted.current && request === generation.current) {
        setDocumentState('ready')
        setSource('')
        setSavedSource('')
        setRevision('')
        setRemoteDocument(null)
      }
      return
    }
    setDocumentState('loading')
    setErrorText('')
    try {
      const document = await guiRead('rules.get', key === 'global' ? {} : { project_id: key })
      if (!mounted.current || request !== generation.current || key !== selectedScope) return
      const draft = preserveDraft ? drafts.current.get(key) : undefined
      setRevision(document.revision)
      setSavedSource(document.source)
      setRemoteDocument(null)
      setDocumentState('ready')
      if (draft) {
        setSource(draft.source)
        setRevision(draft.revision)
        setSavedSource(draft.originalSource)
      } else {
        setSource(document.source)
        setSavedSource(document.source)
        drafts.current.set(key, { source: document.source, revision: document.revision, originalSource: document.source })
      }
    } catch {
      if (!mounted.current || request !== generation.current || key !== selectedScope) return
      setDocumentState('failed')
      setErrorText(copy[languageRef.current].failed)
    }
  }, [selectedScope])

  useEffect(() => {
    mounted.current = true
    return () => {
      mounted.current = false
      generation.current += 1
      drafts.current.clear()
      sourceRef.current = ''
    }
  }, [currentAccountId])

  useEffect(() => {
    if (open && capabilityState === 'loading') void loadCapabilitiesRef.current?.(false)
  }, [open, capabilityState])

  useEffect(() => {
    if (capabilityState !== 'ready' || !canRead) return
    const request = ++generation.current
    clearValidation()
    setRemoteDocument(null)
    void readDocument(selectedScope, request, true)
  }, [selectedScope, capabilityState, canRead, clearValidation, readDocument])

  const updateSource = (nextSource: string) => {
    setSource(nextSource)
    sourceRef.current = nextSource
    drafts.current.set(selectedScope, { source: nextSource, revision, originalSource: savedSource })
    clearValidation()
    setRemoteDocument(null)
    setErrorText('')
    setRecoveryReference('')
    setFormError('')
    setShowTemplateConfirmation(false)
    setShowReloadConfirmation(false)
  }

  const changeScope = (nextScope: string) => {
    if (nextScope === selectedScope) return
    drafts.current.set(selectedScope, { source, revision, originalSource: savedSource })
    setSelectedScope(nextScope)
    setSource('')
    setSavedSource('')
    setRevision('')
    setDocumentState('loading')
    setRemoteDocument(null)
    setErrorText('')
    setRecoveryReference('')
    clearValidation()
    setShowTemplateConfirmation(false)
    setShowReloadConfirmation(false)
  }

  const refreshSelected = useCallback(async (preserveDraft: boolean): Promise<RulesDocument | null> => {
    const request = ++generation.current
    setErrorText('')
    try {
      const [nextCapabilities, nextCreation] = await Promise.all([guiRead('rules.capabilities'), guiCreationStatus()])
      if (!mounted.current || request !== generation.current) return null
      setCapabilities(nextCapabilities)
      setCreationStatus(nextCreation)
      setCapabilityState('ready')
      setCreationState('ready')
      const scopePermissions = selectedScope === 'global' ? nextCapabilities.global : nextCapabilities.projects.find((project) => project.id === selectedScope)
      if (!scopePermissions?.read) {
        drafts.current.delete(selectedScope)
        clearValidation()
        setSource('')
        setSavedSource('')
        setRevision('')
        setRemoteDocument(null)
        setDocumentState('ready')
        setErrorText(t.denied)
        return null
      }
      const document = await guiRead('rules.get', selectedScope === 'global' ? {} : { project_id: selectedScope })
      if (!mounted.current || request !== generation.current) return null
      clearValidation()
      if (preserveDraft && sourceRef.current !== document.source) {
        setRemoteDocument(document)
        setDocumentState('ready')
        setNoticeText(t.verifiedDifferent)
      } else {
        setRemoteDocument(null)
        setRevision(document.revision)
        setSavedSource(document.source)
        setSource(document.source)
        sourceRef.current = document.source
        drafts.current.set(selectedScope, { source: document.source, revision: document.revision, originalSource: document.source })
        setDocumentState('ready')
        setNeedsRefresh(false)
        setNoticeText(t.verifiedSame)
      }
      return document
    } catch {
      if (!mounted.current || request !== generation.current) return null
      setCapabilityState('failed')
      setCreationState('failed')
      setDocumentState('failed')
      setErrorText(t.failed)
      return null
    }
  }, [clearValidation, selectedScope, t.denied, t.failed, t.verifiedDifferent, t.verifiedSame])

  const checkMutationAccess = useCallback(async (scopeKey: ScopeKey, action: 'validate' | 'save'): Promise<boolean> => {
    const [freshCapabilities, freshCreation] = await Promise.all([guiRead('rules.capabilities'), guiCreationStatus()])
    if (!mounted.current) return false
    setCapabilities(freshCapabilities)
    capabilitiesRef.current = freshCapabilities
    setCreationStatus(freshCreation)
    setCapabilityState('ready')
    setCreationState('ready')
    const current = scopeKey === 'global' ? freshCapabilities.global : freshCapabilities.projects.find((project) => project.id === scopeKey)
    if (!current?.read || !current[action]) {
      await refreshSelected(true)
      if (mounted.current && scopeKey === selectedScope) setErrorText(t.unavailable)
      return false
    }
    if (freshCreation.pending) {
      setErrorText(t.pending)
      return false
    }
    return true
  }, [refreshSelected, selectedScope, t.pending, t.unavailable])

  const beginWork = useCallback((): boolean => {
    if (busy || operationRunning.current || !beginOperation()) return false
    operationRunning.current = true
    setWorking(true)
    setErrorText('')
    return true
  }, [beginOperation, busy])

  const endWork = useCallback(() => {
    if (!operationRunning.current) return
    operationRunning.current = false
    setWorking(false)
    endOperation()
  }, [endOperation])

  const validateDraft = async () => {
    if (!canValidate || sourceBytes > MAX_RULES_BYTES || !beginWork()) return
    const snapshotSource = source
    const snapshotScope = selectedScope
    const snapshotRevision = revision
    const request = generation.current
    try {
      if (!await checkMutationAccess(snapshotScope, 'validate')) return
      const result = await guiRulesManage('rules.validate', snapshotScope === 'global' ? { source: snapshotSource } : { project_id: snapshotScope, source: snapshotSource })
      if (!mounted.current || request !== generation.current || snapshotScope !== selectedScope || sourceRef.current !== snapshotSource) return
      setValidation({ ...result, source: snapshotSource, scopeKey: snapshotScope })
      setRevision(snapshotRevision)
      setNoticeText('')
      setErrorText('')
    } catch (error) {
      if (!mounted.current) return
      if (isRevisionConflict(error)) setErrorText(t.conflict)
      else setErrorText(t.error)
      setValidation(null)
    } finally {
      endWork()
    }
  }

  const confirmSave = async () => {
    if (!validationMatches || !canSave || !beginWork()) return
    const snapshotSource = source
    const snapshotScope = selectedScope
    const snapshotRevision = revision
    const request = generation.current
    setShowSaveConfirmation(false)
    let uncertain = false
    try {
      if (!await checkMutationAccess(snapshotScope, 'save')) return
      setNeedsRefresh(true)
      const saved = await guiRulesManage('rules.save', snapshotScope === 'global'
        ? { source: snapshotSource, if_revision: snapshotRevision }
        : { project_id: snapshotScope, source: snapshotSource, if_revision: snapshotRevision })
      if (!mounted.current || request !== generation.current || snapshotScope !== selectedScope) return
      setSource(saved.source)
      sourceRef.current = saved.source
      setSavedSource(saved.source)
      setRevision(saved.revision)
      drafts.current.set(snapshotScope, { source: saved.source, revision: saved.revision, originalSource: saved.source })
      setValidation(null)
      setNeedsRefresh(false)
      setNoticeText(t.saveSuccess)
      try {
        await onRulesChanged()
      } catch {
        setNeedsRefresh(true)
        setNoticeText(t.savedButRefreshFailed)
      }
    } catch (error) {
      if (!mounted.current) return
      if (getFault(error).code === 'RULES_BUSY') {
        setErrorText(t.fileBusy)
        setNeedsRefresh(false)
      } else if (isRevisionConflict(error)) {
        setErrorText(t.conflict)
        setNeedsRefresh(false)
        setValidation(null)
        const fault = getFault(error)
        setRecoveryReference(fault.code === 'RULES_CHANGED' ? fault.message.slice(0, 500) : '')
      } else {
        uncertain = true
        setErrorText(t.saveUncertain)
        setNeedsRefresh(true)
      }
    } finally {
      if (mounted.current && request === generation.current) {
        setShowSaveConfirmation(false)
        if (uncertain) setValidation(null)
      }
      endWork()
    }
  }

  const addRule = (event: FormEvent) => {
    event.preventDefault()
    if (!canRead || !ruleId.trim() || !operation || sourceBytes > MAX_RULES_BYTES) {
      setFormError(t.fieldRequired)
      return
    }
    let conditionValue: unknown
    if (conditionField.trim() && conditionOperator !== 'exists') {
      if (!conditionJson.trim()) {
        setFormError(t.conditionInvalid)
        return
      }
      try { conditionValue = JSON.parse(conditionJson) } catch { setFormError(t.conditionInvalid); return }
    }
    const draft: RuleDraft = {
      id: ruleId.trim(),
      effect,
      action: operation,
      projectId: selectedScope === 'global' ? subjectProjects.trim() || undefined : selectedScope,
      scope: targetScope === '' ? 'global' : targetScope === 'boards' ? 'board' : 'thread',
      targetId: targetScope && targetId.trim() ? targetId.trim() : undefined,
      condition: conditionField.trim() ? { field: conditionField.trim(), op: conditionOperator, ...(conditionOperator === 'exists' ? {} : { value: conditionValue }) } : undefined,
    }
    try {
      updateSource(appendRuleDraft(source, draft))
      setRuleId('')
      setEffect('allow')
      setOperation('')
      setSubjectProjects('')
      setTargetScope('')
      setTargetId('')
      setConditionField('')
      setConditionOperator('eq')
      setConditionJson('')
      setFormError('')
      setNoticeText('')
    } catch (error) {
      setFormError(error instanceof Error && error.message ? error.message : t.error)
    }
  }

  const insertTemplate = () => {
    if (source.length > 0 && source !== DEFAULT_RULES_TEMPLATE) setShowTemplateConfirmation(true)
    else applyTemplate()
  }
  const applyTemplate = () => {
    updateSource(DEFAULT_RULES_TEMPLATE)
    setShowTemplateConfirmation(false)
  }

  const refreshForVerify = async () => {
    const result = await refreshSelected(true)
    if (result) setNeedsRefresh(false)
  }

  const inspectRemoteForReload = async () => {
    const document = await refreshSelected(true)
    if (document) setShowReloadConfirmation(true)
  }

  const reloadRemote = () => {
    if (!remoteDocument) return
    setSource(remoteDocument.source)
    sourceRef.current = remoteDocument.source
    setSavedSource(remoteDocument.source)
    setRevision(remoteDocument.revision)
    drafts.current.set(selectedScope, { source: remoteDocument.source, revision: remoteDocument.revision, originalSource: remoteDocument.source })
    setRemoteDocument(null)
    setShowReloadConfirmation(false)
    setNeedsRefresh(false)
    setErrorText('')
    clearValidation()
  }

  const handleTabKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight' && event.key !== 'Home' && event.key !== 'End') return
    event.preventDefault()
    const next = event.key === 'Home' || event.key === 'ArrowLeft' ? 'text' : event.key === 'End' || event.key === 'ArrowRight' ? 'form' : editorMode
    setEditorMode(next)
    document.getElementById(next === 'text' ? 'rules-tab-text' : 'rules-tab-form')?.focus()
  }

  const templateDialog = showTemplateConfirmation && <div className="rules-inline-confirm" role="alertdialog" aria-labelledby="rules-template-title">
    <h4 id="rules-template-title">{t.templateConfirmTitle}</h4><p>{t.templateConfirm}</p>
    <div className="rules-actions"><button className="button button-danger" type="button" onClick={applyTemplate}>{t.replace}</button><button className="button button-secondary" type="button" onClick={() => setShowTemplateConfirmation(false)}>{t.cancel}</button></div>
  </div>
  const reloadDialog = showReloadConfirmation && <div className="rules-inline-confirm" role="alertdialog" aria-labelledby="rules-reload-title">
    <h4 id="rules-reload-title">{t.reloadTitle}</h4><p>{t.reloadBody}</p>
    <div className="rules-actions"><button className="button button-danger" type="button" onClick={reloadRemote}>{t.reloadConfirm}</button><button className="button button-secondary" type="button" onClick={() => setShowReloadConfirmation(false)}>{t.cancel}</button></div>
  </div>
  const saveDialog = showSaveConfirmation && <div className="rules-inline-confirm" role="alertdialog" aria-labelledby="rules-save-confirmation">
    <h4 id="rules-save-confirmation">{t.saveTitle}</h4><p>{t.saveScope(scopeName(selectedScope, capabilities, language))}</p><p>{t.saveImpact}</p>
    <div className="rules-actions"><button className="button button-primary" type="button" onClick={() => void confirmSave()} disabled={working || !validationMatches || !canSave}>{t.confirmSave}</button><button className="button button-secondary" type="button" onClick={() => setShowSaveConfirmation(false)} disabled={working}>{t.cancel}</button></div>
  </div>

  return <details className="rules-editor-disclosure" open={open} onToggle={(event) => setOpen(event.currentTarget.open)}>
    <summary data-testid="rules-editor-disclosure">{t.title}<span aria-hidden="true">{open ? '−' : '+'}</span></summary>
    <section className="rules-editor" aria-label={t.title}>
      {capabilityState === 'loading' && <p role="status">{t.loading}</p>}
      {capabilityState === 'failed' && <div className="rules-message rules-error" role="alert"><p>{errorText || t.failed}</p><button className="button button-secondary" type="button" onClick={() => void loadCapabilities(true)}>{t.retry}</button></div>}
      {capabilityState === 'ready' && <>
        {!allowedScopes.length && <p className="rules-message" role="status" data-testid="rules-scope-denied">{t.denied}</p>}
        {allowedScopes.length > 0 && <>
          <label className="rules-field"><span>{t.scope}</span><select data-testid="rules-scope-select" value={selectedScope} onChange={(event) => changeScope(event.target.value)} disabled={busy || working}>
            {allowedScopes.map((scope) => <option key={scope.key} value={scope.key}>{scope.label}</option>)}
          </select></label>
          {!canRead && <p className="rules-message" role="alert" data-testid="rules-scope-denied">{t.denied}</p>}
          {canRead && <>
            <RulesRecoveryPanel key={`${currentAccountId}:${selectedScope}`} language={language} accountId={currentAccountId} scope={selectedScope} draft={source} original={revision ? savedSource : null} canSave={canSave} blocked={busy || working || creationState !== 'ready' || pendingCreation} beginOperation={beginOperation} endOperation={endOperation} onRecovered={async () => { await refreshSelected(true); await onRulesChanged() }} />
            {documentState === 'loading' && <p role="status">{t.loading}</p>}
            {documentState === 'failed' && <div className="rules-message rules-error" role="alert"><p>{errorText || t.failed}</p><button className="button button-secondary" type="button" onClick={() => void loadCapabilities(true)} disabled={busy || working}>{t.retry}</button></div>}
            {documentState === 'ready' && <>
              {creationState === 'ready' && pendingCreation && <p className="rules-message" role="status">{t.pending}</p>}
              {creationState === 'failed' && <p className="rules-message rules-error" role="alert">{t.pendingCheckFailed}</p>}
              {!!errorText && <p className="rules-message rules-error" role="alert">{errorText}</p>}
              {!!recoveryReference && <p className="rules-message" role="status">{t.recoveryReference} <code>{recoveryReference}</code></p>}
              {!!noticeText && <p className="rules-message" role="status">{noticeText}</p>}
              {needsRefresh && <button className="button button-secondary" type="button" data-testid="rules-verify-save" onClick={() => void refreshForVerify()} disabled={busy || working}>{t.verify}</button>}
              {errorText === t.conflict && <button className="button button-secondary" type="button" data-testid="rules-reload-conflict" onClick={() => void inspectRemoteForReload()} disabled={busy || working}>{t.reload}</button>}
              <div className="rules-editor-tabs" role="tablist" aria-label={t.tabs} data-testid="rules-editor-tabs" onKeyDown={handleTabKeyDown}>
                <button id="rules-tab-text" type="button" role="tab" data-testid="rules-tab-text" aria-selected={editorMode === 'text'} aria-controls="rules-panel-text" tabIndex={editorMode === 'text' ? 0 : -1} onClick={() => setEditorMode('text')}>{t.text}</button>
                <button id="rules-tab-form" type="button" role="tab" data-testid="rules-tab-form" aria-selected={editorMode === 'form'} aria-controls="rules-panel-form" tabIndex={editorMode === 'form' ? 0 : -1} onClick={() => setEditorMode('form')}>{t.form}</button>
              </div>
              {editorMode === 'text' ? <div id="rules-panel-text" role="tabpanel" aria-labelledby="rules-tab-text" className="rules-panel">
                <label className="rules-field"><span>{t.source}</span><textarea data-testid="rules-source" value={source} onChange={(event) => updateSource(event.target.value)} spellCheck={false} disabled={busy || working || !permissions?.save} aria-describedby="rules-size" /></label>
                <div className="rules-source-footer"><span id="rules-size" className={sourceBytes > MAX_RULES_BYTES ? 'rules-error-text' : ''}>{t.size(sourceBytes)}</span><button className="text-button" data-testid="rules-insert-example" type="button" onClick={insertTemplate} disabled={busy || working || !permissions?.save}>{t.template}</button></div>
                {sourceBytes > MAX_RULES_BYTES && <p className="rules-error-text" role="alert">{t.tooLarge}</p>}
                {templateDialog}
              </div> : <div id="rules-panel-form" role="tabpanel" aria-labelledby="rules-tab-form" className="rules-panel">
                <p className="rules-guide">{t.formGuide}</p>
                {selectedScope !== 'global' && <p className="rules-guide">{t.projectReadOnly}</p>}
                {!source.includes('swarm-rules') && <button className="text-button" type="button" onClick={() => { setEditorMode('text'); if (!source) applyTemplate() }} disabled={busy || working || !permissions?.save}>{t.startWithTemplate}</button>}
                <form className="rules-form" onSubmit={addRule}>
                  <div className="rules-form-grid">
                    <label className="rules-field"><span>{t.id}</span><input data-testid="rules-id" value={ruleId} onChange={(event) => setRuleId(event.target.value)} disabled={busy || working || !permissions?.save} /></label>
                    <label className="rules-field"><span>{t.effect}</span><select data-testid="rules-effect" value={effect} onChange={(event) => setEffect(event.target.value as 'allow' | 'deny')} disabled={busy || working || !permissions?.save}><option value="allow">{t.allow}</option><option value="deny">{t.deny}</option></select></label>
                    <label className="rules-field rules-form-wide"><span>{t.operation}</span><select data-testid="rules-operation" value={operation} onChange={(event) => setOperation(event.target.value)} disabled={busy || working || !permissions?.save}><option value="">{t.chooseOperation}</option>{(capabilities?.actions ?? []).map((action) => <option key={action} value={action}>{action}</option>)}</select></label>
                    <label className="rules-field rules-form-wide"><span>{t.projectId}</span><input data-testid="rules-subject-project-id" value={selectedScope === 'global' ? subjectProjects : selectedScope} onChange={(event) => setSubjectProjects(event.target.value)} disabled={busy || working || !permissions?.save || selectedScope !== 'global'} /><small>{t.projectIdHint}</small></label>
                    <label className="rules-field"><span>{t.targetScope}</span><select data-testid="rules-target-scope" value={targetScope} onChange={(event) => setTargetScope(event.target.value as '' | 'boards' | 'threads')} disabled={busy || working || !permissions?.save}><option value="">{t.noTarget}</option><option value="boards">{t.board}</option><option value="threads">{t.thread}</option></select></label>
                    {targetScope && <label className="rules-field"><span>{t.targetId}</span><input data-testid="rules-target-id" value={targetId} onChange={(event) => setTargetId(event.target.value)} disabled={busy || working || !permissions?.save} /></label>}
                    <label className="rules-field"><span>{t.conditionField}</span><input data-testid="rules-condition-field" value={conditionField} onChange={(event) => setConditionField(event.target.value)} disabled={busy || working || !permissions?.save} /></label>
                    {conditionField && <>
                      <label className="rules-field"><span>{t.conditionOperator}</span><select data-testid="rules-condition-operator" value={conditionOperator} onChange={(event) => setConditionOperator(event.target.value as ConditionOperator)} disabled={busy || working || !permissions?.save}>{['eq', 'ne', 'in', 'contains', 'lt', 'lte', 'gt', 'gte', 'exists'].map((value) => <option key={value} value={value}>{value}</option>)}</select></label>
                      {conditionOperator !== 'exists' && <label className="rules-field rules-form-wide"><span>{t.conditionJson}</span><textarea data-testid="rules-condition-json" value={conditionJson} onChange={(event) => setConditionJson(event.target.value)} disabled={busy || working || !permissions?.save} /></label>}
                    </>}
                  </div>
                  {!!formError && <p className="rules-error-text" role="alert">{formError}</p>}
                  <button className="button button-secondary" data-testid="rules-add-rule" type="submit" disabled={busy || working || !permissions?.save || sourceBytes > MAX_RULES_BYTES}>{t.addRule}</button>
                </form>
              </div>}
              <div className="rules-actions">
                <button className="button button-secondary" data-testid="rules-validate" type="button" onClick={() => void validateDraft()} disabled={operationBlocked || sourceBytes > MAX_RULES_BYTES}>{working ? t.validating : t.validate}</button>
                <button className="button button-primary" data-testid="rules-save" type="button" data-disabled-reason={saveBlockedReason || undefined} aria-describedby={saveBlockedReason ? 'rules-save-reason' : undefined} onClick={() => setShowSaveConfirmation(true)} disabled={busy || working || needsRefresh || !canSave || !validationMatches || dirty === false || sourceBytes > MAX_RULES_BYTES}>{working ? t.saving : t.save}</button>
              </div>
              {saveBlockedReason && <p className="rules-save-reason" id="rules-save-reason" data-testid="rules-save-reason">{saveBlockedReason}</p>}
              {!!validation && validation.source === source && validation.scopeKey === selectedScope && <section className={`rules-validation ${validation.valid ? 'is-valid' : 'is-invalid'}`} data-testid="rules-validation-summary" aria-label={t.validationTitle}>
                <h4>{t.validationTitle} · {validation.valid ? t.valid : t.invalid}</h4>
                <p>{validation.message}</p>
                <p>{t.validationContext(scopeName(selectedScope, capabilities, language))} · {t.validatedSource}</p>
                <details className="rules-source-snapshot" data-testid="rules-validated-source"><summary>{t.sourceSnapshot}</summary><pre>{validation.source}</pre></details>
                <strong>{t.actions}</strong>
                {validation.actions.length ? <ul>{validation.actions.map((action) => <li key={action}><code>{action}</code></li>)}</ul> : <p>{t.noActions}</p>}
              </section>}
              {saveDialog}
              {reloadDialog}
              {remoteDocument && <details className="rules-source-snapshot" data-testid="rules-remote-source"><summary>{t.remoteSource}</summary><pre>{remoteDocument.source}</pre></details>}
              {capabilities && <RulesDiagnostics
                key={`${currentAccountId}:${selectedScope}`}
                capabilities={capabilities}
                currentAccountId={currentAccountId}
                busy={busy || working}
                language={language}
                rulesProjectId={selectedScope === 'global' ? null : selectedScope}
                beginOperation={beginOperation}
                endOperation={endOperation}
              />}
            </>}
          </>}
        </>}
      </>}
    </section>
  </details>
}
