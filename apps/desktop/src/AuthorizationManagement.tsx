import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent } from 'react'
import useSettingsCapabilityGuard from './useSettingsCapabilityGuard'
import { trackOperation } from './operationStatus'
import { guiAuthorizationManage, guiCreationStatus, guiRead } from './bridge'
import type {
  Account,
  AuthorizationCapabilities,
  AuthorizationManagementArgs,
  AuthorizationManageOperation,
  CreationStatus,
  ManagementCapabilities,
  PermissionRule,
  PermissionScope,
  PermissionSetArgs,
  Role,
} from './types'

type Language = 'en' | 'ja'
type LoadState = 'loading' | 'ready' | 'failed'
type Decision = PermissionSetArgs['decision']
type PendingChange =
  | { kind: 'role.create'; name: string }
  | { kind: 'role.assign'; account: Account; role: Role; assigned: boolean }
  | { kind: 'permission.set'; args: PermissionSetArgs[]; description: string }

const copy = {
  en: {
    title: 'Authorization management', loading: 'Loading authorization data…', loadError: 'Could not load authorization data. Changes are blocked until refresh succeeds.', retry: 'Refresh authorization', refreshing: 'Refreshing…',
    unavailable: 'No authorization management permissions are available.', pending: 'A conversation creation request is pending. Finish or discard it before changing authorization.', statusFailed: 'Could not check pending creation. Changes are blocked until refresh succeeds.',
    roles: 'Roles', createRole: 'Create role', roleName: 'Role name', roleCreateSubmit: 'Review role creation', noRoles: 'No roles are available.', roleCreatePreview: (name: string) => `Create role “${name}”`,
    members: 'Account memberships', noAccounts: 'No manageable accounts are available.', assigned: 'Assigned', notAssigned: 'Not assigned', assign: 'Assign', unassign: 'Unassign',
    permissions: 'Explicit permissions', subjectType: 'Subject type', account: 'Account', role: 'Role', subject: 'Subject', chooseSubject: 'Choose a subject', noPermissionSubjects: 'No account or role is available for permission management.',
    permissionSemantics: 'Thread rules override Board and global rules. At the same scope, denial wins. Inherit removes the explicit rule.',
    operation: 'Operation', chooseOperation: 'Choose an operation', scope: 'Scope', global: 'Global', board: 'Board', thread: 'Thread', targetId: 'Target ID',
    decision: 'Decision', allow: 'Allow', deny: 'Deny', inherit: 'Inherit', explicitRules: 'Explicit rules', noRules: 'No explicit rules; permissions inherit from the applicable role and scope.', permissionReadUnavailable: 'This account can set permissions but cannot read existing rules.',
    changeSummary: 'Review this authorization change', confirm: 'Confirm change', cancel: 'Cancel', confirmCreate: 'Create role', confirmAssignment: (assigned: boolean) => assigned ? 'Assign role' : 'Unassign role', confirmPermission: 'Save permission',
    changed: 'Authorization change saved.', uncertain: 'The result could not be confirmed. Refresh authorization data before another change.', staleGrant: 'Your authorization changed. The pending confirmation was cleared; review the current permissions before making another change.',
    invalidRoleName: 'Enter a role name.', invalidSubject: 'Choose an account or role.', invalidOperation: 'Choose an operation and a supported scope.', invalidTarget: 'Enter the target ID for this scope.',
    pendingChanged: 'A conversation creation request is pending. Resolve it before changing authorization.',
    accountLabel: (account: Account) => `${account.name} · ${account.id}`, roleLabel: (role: Role) => `${role.name} · ${role.id}`,
    permissionDescription: (subject: string, operation: string, scope: string, target: string, decision: string) => `${subject}: ${operation} · ${scope}${target ? ` · ${target}` : ''} → ${decision}`,
  },
  ja: {
    title: '権限管理', loading: '権限情報を読み込み中…', loadError: '権限情報を読み込めません。更新が成功するまで変更できません。', retry: '権限情報を更新', refreshing: '更新中…',
    unavailable: '利用できる権限管理操作がありません。', pending: '会話の作成要求が保留中です。権限変更の前に完了または破棄してください。', statusFailed: '保留中の作成要求を確認できません。更新が成功するまで変更できません。',
    roles: 'ロール', createRole: 'ロールを作成', roleName: 'ロール名', roleCreateSubmit: '作成内容を確認', noRoles: '利用できるロールはありません。', roleCreatePreview: (name: string) => `ロール「${name}」を作成`,
    members: 'アカウントのロール割り当て', noAccounts: '管理対象のアカウントはありません。', assigned: '割り当て済み', notAssigned: '未割り当て', assign: '割り当て', unassign: '割り当てを解除',
    permissions: '明示的な権限', subjectType: '対象の種類', account: 'アカウント', role: 'ロール', subject: '対象', chooseSubject: '対象を選択', noPermissionSubjects: '権限を管理できるアカウントまたはロールがありません。',
    permissionSemantics: 'スレッドのルールはボードと全体のルールより優先されます。同じ範囲では拒否が優先されます。「継承」は明示的なルールを削除します。',
    operation: '操作', chooseOperation: '操作を選択', scope: '範囲', global: '全体', board: 'ボード', thread: 'スレッド', targetId: '対象ID',
    decision: '判定', allow: '許可', deny: '拒否', inherit: '継承', explicitRules: '明示的なルール', noRules: '明示的なルールはありません。適用されるロールと範囲から権限を継承します。', permissionReadUnavailable: '権限を設定できますが、既存ルールを読み取る権限がありません。',
    changeSummary: '権限変更を確認', confirm: '変更を確定', cancel: 'キャンセル', confirmCreate: 'ロールを作成', confirmAssignment: (assigned: boolean) => assigned ? 'ロールを割り当て' : '割り当てを解除', confirmPermission: '権限を保存',
    changed: '権限変更を保存しました。', uncertain: '変更結果を確認できません。次の変更の前に権限情報を更新してください。', staleGrant: '権限が変更されました。確認内容を取り消しました。現在の権限を確認してから、もう一度変更してください。',
    invalidRoleName: 'ロール名を入力してください。', invalidSubject: 'アカウントまたはロールを選択してください。', invalidOperation: '操作と対応する範囲を選択してください。', invalidTarget: 'この範囲の対象IDを入力してください。',
    pendingChanged: '会話の作成要求が保留中です。権限変更の前に解決してください。',
    accountLabel: (account: Account) => `${account.name} · ${account.id}`, roleLabel: (role: Role) => `${role.name} · ${role.id}`,
    permissionDescription: (subject: string, operation: string, scope: string, target: string, decision: string) => `${subject}: ${operation} · ${scope}${target ? ` · ${target}` : ''} → ${decision}`,
  },
} as const

function permissionOperationLabel(operation: string, language: Language): string {
  const labels: Record<string, readonly [string, string]> = {
    read: ['Read conversations', '会話を読む'],
    'board.create': ['Create Boards', 'ボードを作成'], 'board.get': ['Read Boards', 'ボードを読む'], 'board.edit': ['Edit Boards', 'ボードを編集'], 'board.delete': ['Delete Boards', 'ボードを削除'], 'board.restore': ['Restore Boards', 'ボードを復元'], 'board.purge': ['Permanently delete Boards', 'Boardを完全削除'],
    'thread.create': ['Create Threads', 'スレッドを作成'], 'thread.get': ['Read Threads', 'スレッドを読む'], 'thread.edit': ['Edit Threads', 'スレッドを編集'], 'thread.delete': ['Delete Threads', 'スレッドを削除'], 'thread.restore': ['Restore Threads', 'スレッドを復元'], 'thread.purge': ['Permanently delete Threads', 'Threadを完全削除'],
    'post.create': ['Create Posts', '投稿を作成'], 'post.get': ['Read Posts', '投稿を読む'], 'post.edit': ['Edit Posts', '投稿を編集'], 'post.delete': ['Delete Posts', '投稿を削除'], 'post.restore': ['Restore Posts', '投稿を復元'], 'post.history': ['Read edit history', '編集履歴を読む'], 'response.create': ['Create Responses', '返信を作成'],
    'account.list': ['List accounts', 'アカウント一覧'], 'account.get': ['Read account information', 'アカウント情報を読む'], 'account.create': ['Add accounts', 'アカウントを追加'], 'account.disable': ['Disable accounts', 'アカウントを無効化'], 'account.reset': ['Reset accounts', 'アカウントをリセット'],
    'role.create': ['Create roles', 'ロールを作成'], 'role.list': ['List roles', 'ロール一覧'], 'role.assign': ['Assign roles', 'ロールを割り当て'], 'permission.set': ['Manage permissions', '権限を管理'], 'permission.list': ['Read permissions', '権限を読む'],
    'project.register': ['Register Projects', 'プロジェクトを登録'], 'project.list': ['List Projects', 'プロジェクト一覧'], 'project.get': ['Read Projects', 'プロジェクトを読む'], 'rules.read': ['Read rules', 'ルールを読む'], 'rules.manage': ['Manage rules', 'ルールを管理'], 'retention.manage': ['Manage data retention', 'データ保持を管理'],
    search: ['Search conversations', '会話を検索'], 'changes.get': ['Read changes', '更新履歴を読む'], 'changes.wait': ['Wait for changes', '更新を待つ'],
    'board.list': ['List Boards', 'ボード一覧'], 'thread.list': ['List Threads', 'スレッド一覧'], 'post.list': ['List Posts', '投稿一覧'], 'post.deleted_body': ['Read deleted content', '削除済みの本文を読む'], 'response.list': ['List Responses', '返信一覧'],
    'project.alias': ['Manage Project aliases', 'プロジェクトの別名を管理'], 'project.root': ['Change Project folder', 'プロジェクトのフォルダーを変更'],
    'rules.check': ['Check permissions', '権限を診断'], 'rules.validate': ['Validate rules', 'ルールを検証'], 'rules.get': ['Read rules', 'ルールを読む'], 'rules.save': ['Save rules', 'ルールを保存'],
    'account.status': ['Read sign-in status', 'ログイン状態を読む'], 'account.password': ['Manage password lock', 'パスワードロックを管理'], 'account.google.begin': ['Begin Google recovery setup', 'Google復旧の設定を開始'], 'account.google.link': ['Link Google recovery', 'Google復旧を連携'], 'account.google.unlink': ['Unlink Google recovery', 'Google復旧の連携を解除'], 'account.logout': ['Log out', 'ログアウト'],
  }
  return labels[operation]?.[language === 'ja' ? 1 : 0] || operation
}

export default function AuthorizationManagement({
  language,
  currentAccountId,
  busy = false,
  beginOperation,
  endOperation,
  onAuthorizationChanged,
  onAccessLost,
}: {
  language: Language
  currentAccountId: string
  busy?: boolean
  beginOperation: () => boolean
  endOperation: () => void
  onAuthorizationChanged: () => Promise<void>
  onAccessLost?: () => void
}) {
  const t = copy[language]
  const [authorization, setAuthorization] = useState<AuthorizationCapabilities | null>(null)
  const [management, setManagement] = useState<ManagementCapabilities | null>(null)
  useSettingsCapabilityGuard(currentAccountId, 'authorization.capabilities', authorization, onAccessLost)
  useSettingsCapabilityGuard(currentAccountId, 'management.capabilities', management, onAccessLost)
  const [accounts, setAccounts] = useState<Account[]>([])
  const [roles, setRoles] = useState<Role[]>([])
  const [loadState, setLoadState] = useState<LoadState>('loading')
  const [creationStatus, setCreationStatus] = useState<CreationStatus | null>(null)
  const [creationState, setCreationState] = useState<LoadState>('loading')
  const [loadError, setLoadError] = useState(false)
  const [needsRefresh, setNeedsRefresh] = useState(false)
  const [mutationBusy, setMutationBusy] = useState(false)
  const [mutationError, setMutationError] = useState('')
  const [notice, setNotice] = useState('')
  const [roleCreateOpen, setRoleCreateOpen] = useState(false)
  const [roleName, setRoleName] = useState('')
  const [pendingChange, setPendingChange] = useState<PendingChange | null>(null)
  const [subjectType, setSubjectType] = useState<'account' | 'role'>('account')
  const [subjectId, setSubjectId] = useState('')
  const [permissionRules, setPermissionRules] = useState<PermissionRule[]>([])
  const [permissionState, setPermissionState] = useState<LoadState>('loading')
  const [selectedScope, setSelectedScope] = useState<PermissionScope>('global')
  const [targetId, setTargetId] = useState('')
  const [permissionDrafts, setPermissionDrafts] = useState<Record<string, Decision>>({})
  const loadGeneration = useRef(0)
  const permissionGeneration = useRef(0)
  const mutationGeneration = useRef(0)
  const mutationRunning = useRef(false)
  const operationAcquired = useRef(false)
  const confirmationRef = useRef<HTMLElement>(null)
  const mounted = useRef(false)
  const accountIdRef = useRef(currentAccountId)
  const subjectIdRef = useRef(subjectId)
  subjectIdRef.current = subjectId
  const subjectTypeRef = useRef(subjectType)
  subjectTypeRef.current = subjectType

  const permissionSubjectOptions = useMemo(() => subjectType === 'account' ? accounts : roles, [accounts, roles, subjectType])

  useEffect(() => {
    if (!pendingChange) return
    confirmationRef.current?.scrollIntoView?.({ block: 'nearest' })
    confirmationRef.current?.querySelector<HTMLButtonElement>('[data-testid="authorization-cancel"]')?.focus({ preventScroll: true })
  }, [pendingChange])

  const loadData = useCallback(async (releaseMutationGuard = false): Promise<boolean> => {
    const request = ++loadGeneration.current
    ++permissionGeneration.current
    if (releaseMutationGuard) setPendingChange(null)
    setLoadState('loading')
    setLoadError(false)
    setCreationState('loading')
    setPermissionState('loading')
    try {
      const [nextAuthorization, nextManagement, nextCreation] = await Promise.all([
        guiRead('authorization.capabilities'),
        guiRead('management.capabilities'),
        guiCreationStatus(),
      ])
      if (!mounted.current || request !== loadGeneration.current) return false
      const [nextAccounts, nextRoles] = await Promise.all([
        nextManagement.account_list ? guiRead('account.list').then((result) => result.items) : Promise.resolve([] as Account[]),
        nextAuthorization.role_list ? guiRead('role.list').then((result) => result.items) : Promise.resolve([] as Role[]),
      ])
      const safeAccounts = nextAccounts.filter((account) => !account.is_admin)
      let nextSubjectType = subjectTypeRef.current
      let nextSubjectId = subjectIdRef.current
      const subjectExists = nextSubjectType === 'account'
        ? safeAccounts.some((account) => account.id === nextSubjectId)
        : nextRoles.some((role) => role.id === nextSubjectId)
      if (!subjectExists) {
        if (safeAccounts.length) { nextSubjectType = 'account'; nextSubjectId = safeAccounts[0].id }
        else if (nextRoles.length) { nextSubjectType = 'role'; nextSubjectId = nextRoles[0].id }
        else nextSubjectId = ''
      }
      let nextPermissionRules: PermissionRule[] = []
      if (nextAuthorization.permission_list && nextSubjectId) {
        const permissionResult = await guiRead('permission.list', { subject_type: nextSubjectType, subject_id: nextSubjectId })
        nextPermissionRules = permissionResult.items
      }
      if (!mounted.current || request !== loadGeneration.current) return false
      setAuthorization(nextAuthorization)
      setManagement(nextManagement)
      setAccounts(safeAccounts)
      setRoles(nextRoles)
      subjectTypeRef.current = nextSubjectType
      subjectIdRef.current = nextSubjectId
      setSubjectType(nextSubjectType)
      setSubjectId(nextSubjectId)
      setPermissionRules(nextPermissionRules)
      setPermissionState('ready')
      setCreationStatus(nextCreation)
      setCreationState('ready')
      setLoadState('ready')
      setLoadError(false)
      if (releaseMutationGuard) setNeedsRefresh(false)
      if (!nextAuthorization.permission_list) {
        setPermissionRules([])
        setPermissionState('ready')
      }
      return true
    } catch {
      if (!mounted.current || request !== loadGeneration.current) return false
      setLoadState('failed')
      setCreationState('failed')
      setPermissionState('failed')
      setLoadError(true)
      setNeedsRefresh(true)
      setAuthorization(null)
      setManagement(null)
      setAccounts([])
      setRoles([])
      setPermissionRules([])
      setSubjectId('')
      subjectIdRef.current = ''
      setPermissionState('failed')
      setPendingChange(null)
      setRoleCreateOpen(false)
      setRoleName('')
      return false
    }
  }, [])

  useEffect(() => {
    const accountChanged = accountIdRef.current !== currentAccountId
    accountIdRef.current = currentAccountId
    if (accountChanged) {
      ++loadGeneration.current
      ++permissionGeneration.current
      ++mutationGeneration.current
      mutationRunning.current = false
      releaseOperation()
      setAuthorization(null)
      setManagement(null)
      setAccounts([])
      setRoles([])
      setPermissionRules([])
      setSubjectId('')
      subjectIdRef.current = ''
      setRoleName('')
      setPendingChange(null)
      setRoleCreateOpen(false)
      setNeedsRefresh(false)
      setMutationBusy(false)
      setMutationError('')
      setNotice('')
    }
    mounted.current = true
    void loadData()
    return () => {
      mounted.current = false
      ++loadGeneration.current
      ++permissionGeneration.current
      ++mutationGeneration.current
      mutationRunning.current = false
      releaseOperation()
    }
  }, [currentAccountId, endOperation, loadData])

  useEffect(() => {
    const supported = authorization?.operations.flatMap((item) => item.scopes) || []
    if (supported.length && !supported.includes(selectedScope)) {
      setSelectedScope(supported[0])
      setTargetId('')
    }
  }, [authorization, selectedScope])

  useEffect(() => {
    setPermissionDrafts({})
    setPendingChange(null)
  }, [permissionRules, currentAccountId, subjectType, subjectId, selectedScope, targetId])

  const canMutate = loadState === 'ready'
    && creationState === 'ready'
    && creationStatus?.pending !== true
    && !busy
    && !mutationBusy
    && !needsRefresh
    && !loadError
  const permissionBlockedReason = !subjectId
    ? language === 'ja' ? '変更するユーザーアカウントまたはロールを選択してください。Admin自身は対象外です。AIの権限は「ルール編集」で設定します。' : 'Choose a human account or role. The primary Admin is excluded; AI permissions are configured in Rules editor.'
    : busy || mutationBusy ? t.refreshing
    : needsRefresh ? t.uncertain
    : loadError || loadState !== 'ready' ? t.loadError
    : creationStatus?.pending ? t.pending
    : creationState !== 'ready' ? t.statusFailed
    : authorization?.permission_list && permissionState !== 'ready' ? permissionState === 'failed' ? t.loadError : t.loading
    : ''
  const available = Boolean(authorization && (
    authorization.role_create
    || authorization.role_list
    || (authorization.role_assign && management?.account_list && authorization.role_list)
    || ((authorization.permission_list || authorization.permission_set) && (management?.account_list || authorization.role_list))
  ))
  const canManageMemberships = Boolean(authorization?.role_assign && roles.length && management?.account_list && accounts.length)
  const permissionSubjectsAvailable = permissionSubjectOptions.length > 0
  const selectedSubject = permissionSubjectOptions.find((item) => item.id === subjectId)
  const supportedScopes = (['global', 'board', 'thread'] as const).filter((scope) => authorization?.operations.some((item) => item.scopes.includes(scope)))
  const visibleOperations = authorization?.operations.filter((item) => item.scopes.includes(selectedScope)) || []
  const normalizedTarget = selectedScope === 'global' ? '' : targetId.trim()

  function savedDecision(operation: string): Decision | undefined {
    if (!authorization?.permission_list || permissionState !== 'ready') return undefined
    return permissionRules.find((rule) => rule.operation === operation && rule.scope === selectedScope && rule.target_id === normalizedTarget)?.decision || 'inherit'
  }

  function chooseDecision(operation: string, decision: Decision) {
    if (!canMutate || !subjectId || (authorization?.permission_list && permissionState !== 'ready')) return
    setPendingChange(null)
    setMutationError('')
    setPermissionDrafts((previous) => {
      const next = { ...previous }
      if (decision === savedDecision(operation)) delete next[operation]
      else next[operation] = decision
      return next
    })
  }

  function selectSubject(nextType: 'account' | 'role', nextId: string) {
    if (busy || mutationBusy) return
    setPendingChange(null)
    setMutationError('')
    subjectTypeRef.current = nextType
    subjectIdRef.current = nextId
    setSubjectType(nextType)
    setSubjectId(nextId)
    setPermissionRules([])
    void readPermissionRules(nextType, nextId)
  }

  function releaseOperation() {
    mutationRunning.current = false
    if (operationAcquired.current) {
      operationAcquired.current = false
      endOperation()
    }
  }

  async function readPermissionRules(nextType: 'account' | 'role', nextId: string) {
    const request = ++permissionGeneration.current
    setPermissionState('loading')
    if (!authorization?.permission_list || !nextId) {
      setPermissionRules([])
      setPermissionState('ready')
      return
    }
    try {
      const result = await guiRead('permission.list', { subject_type: nextType, subject_id: nextId })
      if (!mounted.current || request !== permissionGeneration.current) return
      setPermissionRules(result.items)
      setPermissionState('ready')
    } catch {
      if (!mounted.current || request !== permissionGeneration.current) return
      setPermissionRules([])
      setPermissionState('failed')
      setNeedsRefresh(true)
    }
  }

  function askRoleCreate(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (!canMutate || !authorization?.role_create) return
    const submittedName = roleName.trim()
    if (!submittedName) {
      setMutationError(t.invalidRoleName)
      return
    }
    setMutationError('')
    setPendingChange({ kind: 'role.create', name: submittedName })
  }

  function askPermissionChange(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (!canMutate || !authorization?.permission_set || (authorization.permission_list && permissionState !== 'ready')) return
    if (!subjectId || (subjectType === 'account' && !accounts.some((item) => item.id === subjectId && !item.is_admin)) || (subjectType === 'role' && !roles.some((item) => item.id === subjectId))) {
      setMutationError(t.invalidSubject)
      return
    }
    const changes = Object.entries(permissionDrafts)
    if (!changes.length || changes.some(([operation]) => !authorization.operations.some((item) => item.operation === operation && item.scopes.includes(selectedScope)))) {
      setMutationError(t.invalidOperation)
      return
    }
    const normalizedTarget = selectedScope === 'global' ? '' : targetId.trim()
    if (selectedScope !== 'global' && !normalizedTarget) {
      setMutationError(t.invalidTarget)
      return
    }
    const args: PermissionSetArgs[] = changes.map(([operation, decision]) => ({ subject_type: subjectType, subject_id: subjectId, operation, scope: selectedScope, target_id: normalizedTarget, decision }))
    const subjectName = selectedSubject?.name || subjectId
    const description = args.map((item) => t.permissionDescription(subjectName, item.operation, t[selectedScope], normalizedTarget, t[item.decision])).join('\n')
    setMutationError('')
    setPendingChange({ kind: 'permission.set', args, description })
  }

  function askRoleAssignment(account: Account, role: Role, assigned: boolean) {
    if (!canMutate || !authorization?.role_assign || account.is_admin) return
    setMutationError('')
    setPendingChange({ kind: 'role.assign', account, role, assigned })
  }

  async function confirmChange() {
    const change = pendingChange
    if (!change || !canMutate || mutationRunning.current) return
    const grantStillAvailable = change.kind === 'role.create'
      ? authorization?.role_create === true
      : change.kind === 'role.assign'
        ? authorization?.role_assign === true
          && !change.account.is_admin
          && accounts.some((account) => account.id === change.account.id && !account.is_admin)
          && roles.some((role) => role.id === change.role.id)
        : authorization?.permission_set === true
          && change.args.every((args) => (args.subject_type === 'account'
            ? accounts.some((account) => account.id === args.subject_id && !account.is_admin)
            : roles.some((role) => role.id === args.subject_id))
            && authorization.operations.some((item) => item.operation === args.operation && item.scopes.includes(args.scope)))
    if (!grantStillAvailable) {
      setPendingChange(null)
      setMutationError(t.staleGrant)
      setNeedsRefresh(true)
      void loadData(true)
      return
    }
    if (!beginOperation()) return
    operationAcquired.current = true
    const request = ++mutationGeneration.current
    mutationRunning.current = true
    setMutationBusy(true)
    setMutationError('')
    setNotice('')
    setNeedsRefresh(true)
    let mutationIssued = false
    let refreshAttempted = false
    try {
      let status: CreationStatus
      try {
        status = await guiCreationStatus()
      } catch {
        if (mounted.current && request === mutationGeneration.current) {
          setCreationState('failed')
          setMutationError(t.statusFailed)
        }
        return
      }
      if (!mounted.current || request !== mutationGeneration.current) return
      setCreationStatus(status)
      setCreationState('ready')
      if (status.pending) {
        setMutationError(t.pendingChanged)
        return
      }
      let operation: AuthorizationManageOperation
      let args: AuthorizationManagementArgs[AuthorizationManageOperation]
      if (change.kind === 'role.create') {
        operation = 'role.create'
        args = { name: change.name }
      } else if (change.kind === 'role.assign') {
        operation = 'role.assign'
        args = { account_id: change.account.id, role_id: change.role.id, assigned: change.assigned }
      } else {
        operation = 'permission.set'
        args = change.args[0]
      }
      mutationIssued = true
      if (change.kind === 'permission.set') {
        await trackOperation('permission.batch.save', async () => { for (const permission of change.args) {
          if (!mounted.current || request !== mutationGeneration.current) throw new Error('Permission save interrupted')
          await guiAuthorizationManage('permission.set', permission)
        } })
      } else await guiAuthorizationManage(operation, args as never)
      if (!mounted.current || request !== mutationGeneration.current) return
      refreshAttempted = true
      await onAuthorizationChanged()
      if (!mounted.current || request !== mutationGeneration.current) return
      setPendingChange(null)
      setRoleCreateOpen(false)
      setRoleName('')
      setNotice(t.changed)
      const refreshed = await loadData(true)
      if (!refreshed && mounted.current && request === mutationGeneration.current) setMutationError(t.uncertain)
    } catch {
      if (mounted.current && request === mutationGeneration.current) {
        setMutationError(t.uncertain)
        setPendingChange(null)
        setNeedsRefresh(true)
      }
      if (mutationIssued && !refreshAttempted) {
        refreshAttempted = true
        try {
          await onAuthorizationChanged()
        } catch {
          // The App callback enters its fail-closed connection state if a fresh snapshot cannot be read.
        }
      }
    } finally {
      if (mounted.current && request === mutationGeneration.current) {
        setMutationBusy(false)
        releaseOperation()
      }
    }
  }

  const assignedAccountIds = useMemo(() => {
    const map = new Map<string, Set<string>>()
    for (const account of accounts) map.set(account.id, new Set(account.role_ids))
    return map
  }, [accounts])

  const roleAssignmentSummary = (account: Account, role: Role) => assignedAccountIds.get(account.id)?.has(role.id) || false

  return <section className="authorization-management" data-testid="authorization-management" aria-labelledby="authorization-title">
    <header className="authorization-heading">
      <h3 id="authorization-title">{t.title}</h3>
      {loadState !== 'loading' && <button className="post-link" type="button" data-testid="authorization-refresh" onClick={() => void loadData(true)} disabled={mutationBusy || busy}>{t.retry}</button>}
    </header>
    {loadState === 'loading' && <p className="authorization-status" data-testid="authorization-loading" role="status">{t.loading}</p>}
    {loadError && <div className="authorization-error"><p data-testid="authorization-error" role="alert">{t.loadError}</p><button className="post-link" type="button" data-testid="authorization-retry" onClick={() => void loadData(true)} disabled={mutationBusy || busy}>{t.retry}</button></div>}
    {loadState === 'ready' && !available && <p className="authorization-status">{t.unavailable}</p>}
    {creationState === 'loading' && loadState === 'ready' && <p className="authorization-status" role="status">{language === 'ja' ? '保留中の作成要求を確認中…' : 'Checking pending creation…'}</p>}
    {creationState === 'failed' && loadState === 'ready' && <p className="authorization-error" role="alert">{t.statusFailed}</p>}
    {creationStatus?.pending && <p className="authorization-pending" data-testid="authorization-pending" role="alert">{t.pending}</p>}
    {mutationError && <p className="authorization-error-text" data-testid="authorization-mutation-error" role="alert">{mutationError}</p>}
    {notice && <p className="authorization-success" data-testid="authorization-notice" role="status">{t.changed}</p>}

    {(authorization?.role_create || authorization?.role_list || canManageMemberships) && <details className="authorization-role-settings">
      <summary>{language === 'ja' ? 'ロール・アカウントの割り当て' : 'Roles & account memberships'}</summary>
    {authorization?.role_create && <div className="authorization-block">
      {!roleCreateOpen
        ? <button className="button button-secondary" type="button" data-testid="authorization-role-create-toggle" onClick={() => { setRoleCreateOpen(true); setMutationError('') }} disabled={!canMutate}>{t.createRole}</button>
        : <form className="authorization-form" data-testid="authorization-role-create-form" onSubmit={askRoleCreate}>
            <label className="field"><span>{t.roleName}</span><input data-testid="authorization-role-name" value={roleName} onChange={(event) => setRoleName(event.target.value)} disabled={!canMutate} /></label>
            <div className="authorization-actions"><button className="button button-secondary" type="button" data-testid="authorization-role-create-cancel" onClick={() => { setRoleCreateOpen(false); setRoleName('') }} disabled={mutationBusy}>{t.cancel}</button><button className="button button-primary" type="submit" data-testid="authorization-role-create-submit" disabled={!canMutate}>{t.roleCreateSubmit}</button></div>
          </form>}
    </div>}

    {authorization?.role_list && <section className="authorization-block" aria-labelledby="authorization-roles-title">
      <h4 id="authorization-roles-title">{t.roles}</h4>
      {roles.length ? <ul className="authorization-role-list" data-testid="authorization-role-list">{roles.map((role) => <li key={role.id}><span>{role.name}</span><code>{role.id}</code></li>)}</ul> : <p className="authorization-status">{t.noRoles}</p>}
    </section>}

    {canManageMemberships && <section className="authorization-block" aria-labelledby="authorization-members-title">
      <h4 id="authorization-members-title">{t.members}</h4>
      {accounts.length === 0 ? <p className="authorization-status">{t.noAccounts}</p> : <div className="authorization-members" data-testid="authorization-members-list">
        {accounts.map((account) => <article className="authorization-account" data-testid={`authorization-account-row-${account.id}`} key={account.id}>
          <strong>{account.name}</strong><code>{account.id}</code>
          <div className="authorization-assignment-list">
            {roles.map((role) => {
              const assigned = roleAssignmentSummary(account, role)
              return <div className="authorization-assignment" key={role.id}>
                <span>{role.name}</span><span className={assigned ? 'authorization-assigned' : 'authorization-unassigned'}>{assigned ? t.assigned : t.notAssigned}</span>
                <button className="post-link" type="button" data-testid={`authorization-role-assignment-${account.id}-${role.id}`} onClick={() => askRoleAssignment(account, role, !assigned)} disabled={!canMutate}>{assigned ? t.unassign : t.assign}</button>
              </div>
            })}
          </div>
        </article>)}
      </div>}
    </section>}

    </details>}

    {(authorization?.permission_list || authorization?.permission_set) && <section className="authorization-block" aria-labelledby="authorization-permissions-title">
      <h4 id="authorization-permissions-title">{t.permissions}</h4>
      <p className="authorization-status" data-testid="authorization-permission-semantics">{t.permissionSemantics}</p>
      <div className="authorization-subject-picker">
        <label className="field"><span>{t.subjectType}</span>
          <select data-testid="authorization-subject-type" value={subjectType} onChange={(event) => { const type = event.target.value as 'account' | 'role'; selectSubject(type, (type === 'account' ? accounts : roles)[0]?.id || '') }} disabled={busy || mutationBusy}>
            <option value="account" disabled={!accounts.length}>{t.account}</option><option value="role" disabled={!roles.length}>{t.role}</option>
          </select>
        </label>
        <label className="field"><span>{t.subject}</span>
          <select data-testid="authorization-subject" value={subjectId} onChange={(event) => selectSubject(subjectType, event.target.value)} disabled={busy || mutationBusy || !permissionSubjectsAvailable}>
            {!permissionSubjectOptions.length && <option value="">{t.chooseSubject}</option>}
            {permissionSubjectOptions.map((item) => <option key={item.id} value={item.id}>{item.name} · {item.id}</option>)}
          </select>
        </label>
      </div>
      {!permissionSubjectsAvailable && <p className="authorization-status">{t.noPermissionSubjects}</p>}
      {authorization.permission_list && subjectId && <section className="authorization-rules" aria-label={t.explicitRules}>
        {permissionState === 'loading' && <p className="authorization-status" role="status">{t.loading}</p>}
        {permissionState === 'failed' && <p className="authorization-error" role="alert">{t.loadError}</p>}
        {permissionState === 'ready' && permissionRules.length === 0 && <p className="authorization-status" data-testid="authorization-no-rules">{t.noRules}</p>}
        {permissionState === 'ready' && permissionRules.length > 0 && !authorization.permission_set && <ul data-testid="authorization-permission-list">{permissionRules.map((rule) => <li key={`${rule.operation}|${rule.scope}|${rule.target_id}`}><span>{rule.operation}</span><span>{t[rule.scope]}</span>{rule.target_id && <code>{rule.target_id}</code>}<strong className={`permission-${rule.decision}`}>{t[rule.decision]}</strong></li>)}</ul>}
      </section>}
      {authorization.permission_set && <form className="authorization-form permission-editor" data-testid="authorization-permission-form" onSubmit={askPermissionChange}>
        {authorization.permission_list && permissionState === 'failed' && <p className="authorization-error" role="alert">{t.loadError}</p>}
        {!authorization.permission_list && <p className="authorization-status">{t.permissionReadUnavailable}</p>}
        <div className="authorization-segments" role="group" aria-label={t.scope}>
          {supportedScopes.map((scope) => <button type="button" key={scope} data-testid={`authorization-scope-${scope}`} aria-pressed={selectedScope === scope} onClick={() => { setPendingChange(null); setSelectedScope(scope); setTargetId('') }} disabled={!canMutate}>{t[scope]}</button>)}
        </div>
        {selectedScope !== 'global' && <label className="field"><span>{t.targetId}</span><input data-testid="authorization-target-id" value={targetId} onChange={(event) => { setPendingChange(null); setTargetId(event.target.value) }} disabled={!canMutate} /></label>}
        <p className="authorization-status">{language === 'ja' ? '各操作の拒否・継承・許可を選び、最後に変更内容を確認します。対象や範囲を変えると未保存の変更は取り消されます。' : 'Choose Deny, Inherit or Allow for each operation, then review your changes. Changing the subject or scope discards unsaved changes.'}</p>
        <div className="permission-matrix" data-testid="authorization-permission-matrix">
          {visibleOperations.map(({ operation }) => {
            const selected = permissionDrafts[operation] ?? savedDecision(operation)
            const dirty = Object.hasOwn(permissionDrafts, operation)
            return <div className={`permission-matrix-row${dirty ? ' permission-matrix-dirty' : ''}`} key={operation} data-testid={`authorization-permission-row-${operation}`}>
              <div className="permission-matrix-label"><strong>{permissionOperationLabel(operation, language)}</strong><code>{operation}</code>{dirty && <span>{language === 'ja' ? '未保存' : 'Unsaved'}</span>}</div>
              <div className="permission-toggle" role="radiogroup" aria-label={permissionOperationLabel(operation, language)}>
                {(['deny', 'inherit', 'allow'] as const).map((option) => <label className={`permission-toggle-option permission-toggle-${option}${selected === option ? ' is-selected' : ''}`} key={option}>
                  <input type="radio" name={`permission-${operation}`} data-testid={`authorization-decision-${operation}-${option}`} data-disabled-reason={permissionBlockedReason || undefined} aria-label={`${permissionOperationLabel(operation, language)}: ${t[option]}`} checked={selected === option} onChange={() => chooseDecision(operation, option)} disabled={!canMutate || !subjectId || (authorization.permission_list && permissionState !== 'ready')} />
                  <span aria-hidden="true">{option === 'deny' ? '×' : option === 'allow' ? '✓' : '−'}</span><span>{t[option]}</span>
                </label>)}
              </div>
            </div>
          })}
        </div>
        <button className="button button-secondary" type="submit" data-testid="authorization-permission-review" disabled={!canMutate || !subjectId || !Object.keys(permissionDrafts).length || (authorization.permission_list && permissionState !== 'ready')}>{language === 'ja' ? `変更内容を確認（${Object.keys(permissionDrafts).length}件）` : `Review changes (${Object.keys(permissionDrafts).length})`}</button>
      </form>}
    </section>}

    {pendingChange && <section ref={confirmationRef} className="authorization-confirmation" data-testid="authorization-confirmation" aria-labelledby="authorization-confirmation-title">
      <h4 id="authorization-confirmation-title">{t.changeSummary}</h4>
      <p className="authorization-change-summary" data-testid="authorization-change-summary">{pendingChange.kind === 'role.create'
        ? t.roleCreatePreview(pendingChange.name)
        : pendingChange.kind === 'role.assign'
          ? `${pendingChange.account.name} · ${pendingChange.account.id} — ${pendingChange.role.name} · ${pendingChange.role.id} — ${pendingChange.assigned ? t.assign : t.unassign}`
          : pendingChange.description}</p>
      {pendingChange.kind === 'permission.set' && pendingChange.args.length > 1 && <p>{language === 'ja' ? '変更を順に保存します。途中で失敗した場合、一部が保存済みの可能性があります。再取得して現在の状態を確認してください。' : 'Changes are saved in order. If a save fails, some changes may already be saved. Refresh to check the current state.'}</p>}
      <div className="authorization-actions"><button className="button button-secondary" type="button" data-testid="authorization-cancel" onClick={() => setPendingChange(null)} disabled={mutationBusy}>{t.cancel}</button><button className="button button-primary" type="button" data-testid="authorization-confirm" onClick={() => void confirmChange()} disabled={!canMutate}>{mutationBusy ? t.refreshing : pendingChange.kind === 'role.create' ? t.confirmCreate : pendingChange.kind === 'role.assign' ? t.confirmAssignment(pendingChange.assigned) : t.confirmPermission}</button></div>
    </section>}
  </section>
}
