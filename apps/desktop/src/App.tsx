import { useCallback, useEffect, useRef, useState, type FormEvent } from 'react'
import { authenticate, cancelPending, getSnapshot, guiRuntimeStart, guiRuntimeStatus, guiRuntimeStop, guiStartupSet, guiStartupStatus, logout, recoverAccount, retryPending, type AuthMode } from './bridge'
import type { GuiFault, GuiSnapshot, RuntimeStatus, StartupStatus } from './types'
import { clearOperationStatuses } from './operationStatus'
import ChangeSavedToast from './ChangeSavedToast'
import AuthorizationManagement from './AuthorizationManagement'
import AccountAccess from './AccountAccess'
import AccountNameInput from './AccountNameInput'
import PasswordLockPanel from './PasswordLockPanel'
import GoogleRecoverySettings, { GoogleRecoveryForm } from './GoogleRecovery'
import AccountManagement from './AccountManagement'
import Workspace from './Workspace'
import { useStartupUpdate, useUpdateNotes } from './updates'
import UpdateSettings from './UpdateSettings'
import BugReportsPanel from './BugReportsPanel'
import RulesEditor from './RulesEditor'
import NotificationSettingsPanel from './NotificationSettingsPanel'
import BackupSettingsPanel from './BackupSettingsPanel'
import RetentionSettingsPanel from './RetentionSettingsPanel'
import ProjectSettingsPanel from './ProjectSettingsPanel'
import { RulesAiGuide } from './SwarmUsageGuides'
import Tabs from './Tabs'
import useDisplaySettings from './useDisplaySettings'
import useNativeWindowTheme from './useNativeWindowTheme'
import { installNativeClosePreferenceListener } from './nativeClosePreferences'

type Language = 'en' | 'ja'
type Theme = 'light' | 'dark'
type ViewState = 'loading' | 'ready' | 'connection-failed' | 'desktop-required'

const copy = {
  en: {
    eyebrow: 'LOCAL CONVERSATION SPACE',
    tagline: 'A calm place for people and agents to work together.',
    language: '日本語',
    theme: 'Dark appearance',
    themeLight: 'Light appearance',
    desktopRequiredTitle: 'Open Swarm desktop',
    desktopRequiredBody: 'Account access is available in the Swarm desktop app.',
    loading: 'Connecting to Swarm…',
    setupTitle: 'Create your administrator account',
    loginTitle: 'Welcome back',
    setupBody: 'Set up the local administrator account to get started.',
    loginBody: 'Sign in to continue to your local workspace.',
    name: 'Account name',
    password: 'Password',
    namePlaceholder: 'Your name',
    passwordPlaceholder: 'Enter your password',
    setupAction: 'Create account',
    skipPassword: 'Skip',
    passwordLater: 'You can set up password locking later.',
    unlockedLoginHint: 'Leave the password blank if password locking is off.',
    loginAction: 'Sign in',
    connectionTitle: 'Can’t reach Swarm right now',
    connectionBody: 'Your local service may still be starting. Try connecting again.',
    retry: 'Retry connection',
    pendingTitle: 'Authentication is waiting to finish',
    pendingBody: 'Retry the saved request to continue. Your password is not needed again.',
    retryPending: 'Retry authentication',
    discardPending: 'Discard saved authentication request',
    discardPendingExplain: 'This cancels the saved request. It cannot roll back an action that has already been committed.',
    discardPendingConfirm: 'Discard request',
    cancel: 'Cancel',
    recoverAdmin: 'Recover Admin account',
    recoveryName: 'Admin account name',
    recoveryCode: 'Recovery code',
    recoveryPassword: 'New password',
    recoverySubmit: 'Recover account',
    recoveryCancel: 'Cancel recovery',
    recoveryFailed: 'Recovery could not be completed. Check the details and try again.',
    recoveryEffect: 'Recovery replaces the Admin password and revokes the current login sessions.',
    discardPendingError: 'Could not discard the saved request. Reconnect and check its status before retrying.',
    accountTitle: 'Your account',
    admin: 'Administrator',
    member: 'Member',
    disabled: 'Disabled',
    accountId: 'Account ID',
    accountStatus: 'Status',
    roles: 'Roles',
    roleCount: (count: number) => `${count} assigned`,
    noRoles: 'No assigned roles',
    settings: 'Settings',
    closeSettings: 'Close settings',
    boardsTitle: 'Your workspace is ready',
    boardsBody: 'Boards will appear here when the conversation view is available.',
    logout: 'Log out',
    localSettings: 'Language and appearance are saved per account. Conversation view is saved on this device.',
    requestFailed: 'Swarm could not complete that request. Try again.',
    invalidName: 'Enter an account name.',
    invalidPassword: 'Enter your password.',
    languageLabel: 'Language',
    appearanceLabel: 'Appearance',
    runtimeTitle: 'Background runtime',
    runtimeStatusLoading: 'Checking runtime status…',
    runtimeRunning: 'Running',
    runtimeStopped: 'Stopped by your request',
    runtimeNotRunning: 'Not running',
    runtimeNotRunningBanner: 'Swarm is not running. Start it to continue using your workspace.',
    runtimeStatusFailed: 'Could not check the runtime status.',
    runtimeStart: 'Start Swarm',
    runtimeStarting: 'Starting…',
    runtimeStop: 'Stop Swarm completely',
    runtimeStopConfirmTitle: 'Stop Swarm completely?',
    runtimeStopConfirmText: 'This stops the background service and rejects new CLI operations. Closing this window alone leaves Swarm running.',
    runtimeStopConfirmAction: 'Stop completely',
    runtimeStopCancel: 'Keep running',
    runtimeStopFailed: 'Could not confirm the complete stop. Check the status before trying again.',
    runtimeStartFailed: 'Could not start the background runtime. Check its status and retry.',
    runtimeRefreshFailed: 'The runtime started, but the workspace did not refresh. Check the status and retry the workspace connection.',
    runtimeRetryStart: 'Start Swarm',
    runtimeStoppedBanner: 'Swarm is completely stopped. Workspace actions are paused until you start the runtime.',
    runtimeCloseNote: 'Closing the desktop window does not stop the background runtime.',
    startupTitle: 'Windows sign-in startup',
    startupToggle: 'Start Swarm when I sign in to Windows',
    startupStatusLoading: 'Checking Windows startup registration…',
    startupDefault: 'Using the default enabled setting',
    startupEnabled: 'Enabled for Windows sign-in',
    startupDisabled: 'Disabled for Windows sign-in',
    startupRegistrationMissing: 'Startup is enabled, but its Windows registration is missing.',
    startupRegistrationUnexpected: 'Startup is disabled, but a Windows registration is still present.',
    startupStatusFailed: 'Could not check the Windows startup setting.',
    startupApply: 'Apply startup setting again',
    startupApplyFailed: 'Could not update the Windows startup registration. Check the displayed setting and retry.',
    startupMismatchFailed: 'Windows startup registration does not match the requested setting. Apply it again to repair the mismatch.',
  },
  ja: {
    eyebrow: 'ローカル会話スペース',
    tagline: '人とエージェントが一緒に作業する場所。',
    language: 'English',
    theme: 'ダーク表示',
    themeLight: 'ライト表示',
    desktopRequiredTitle: 'Swarm デスクトップを開いてください',
    desktopRequiredBody: 'アカウント機能はSwarmデスクトップアプリで利用できます。',
    loading: 'Swarmに接続しています…',
    setupTitle: '管理者アカウントを作成',
    loginTitle: 'おかえりなさい',
    setupBody: '最初にローカル管理者アカウントを設定してください。',
    loginBody: 'ローカルワークスペースにサインインしてください。',
    name: 'アカウント名',
    password: 'パスワード',
    namePlaceholder: '名前を入力',
    passwordPlaceholder: 'パスワードを入力',
    setupAction: 'アカウントを作成',
    skipPassword: 'スキップ',
    passwordLater: 'パスワードによるロックは後からでも設定できます。',
    unlockedLoginHint: 'パスワードによるロックが無効の場合は、パスワードを空欄にしてください。',
    loginAction: 'サインイン',
    connectionTitle: 'Swarmに接続できません',
    connectionBody: 'ローカルサービスが起動中の場合があります。もう一度接続してください。',
    retry: '接続を再試行',
    pendingTitle: '認証要求の完了待ちです',
    pendingBody: '保存済みの要求を再試行してください。パスワードの再入力は不要です。',
    retryPending: '認証を再試行',
    discardPending: '保存済みの認証要求を破棄',
    discardPendingExplain: '保存済みの要求を取り消します。すでに確定した操作を取り消すことはできません。',
    discardPendingConfirm: '要求を破棄',
    cancel: 'キャンセル',
    recoverAdmin: '管理者アカウントを復旧',
    recoveryName: '管理者アカウント名',
    recoveryCode: '復旧コード',
    recoveryPassword: '新しいパスワード',
    recoverySubmit: 'アカウントを復旧',
    recoveryCancel: '復旧をキャンセル',
    recoveryFailed: '復旧できませんでした。入力内容を確認してもう一度お試しください。',
    recoveryEffect: '復旧すると管理者パスワードを変更し、現在のログインセッションを失効させます。',
    discardPendingError: '保存済み要求を破棄できません。再接続して状態を確認してから再試行してください。',
    accountTitle: 'アカウント',
    admin: '管理者',
    member: 'メンバー',
    disabled: '無効',
    accountId: 'アカウントID',
    accountStatus: '状態',
    roles: 'ロール',
    roleCount: (count: number) => `${count}件`,
    noRoles: '割り当て済みロールなし',
    settings: '設定',
    closeSettings: '設定を閉じる',
    boardsTitle: 'ワークスペースの準備ができました',
    boardsBody: '会話画面が利用可能になると、ここにボードが表示されます。',
    logout: 'ログアウト',
    localSettings: '言語と表示はアカウントごとに保存します。会話の表示形式はこの端末に保存します。',
    requestFailed: 'Swarmが要求を完了できませんでした。もう一度お試しください。',
    invalidName: 'アカウント名を入力してください。',
    invalidPassword: 'パスワードを入力してください。',
    languageLabel: '言語',
    appearanceLabel: '表示',
    runtimeTitle: 'バックグラウンドサービス',
    runtimeStatusLoading: 'サービスの状態を確認しています…',
    runtimeRunning: '稼働中',
    runtimeStopped: '完全停止中',
    runtimeNotRunning: '未起動',
    runtimeNotRunningBanner: 'サービスが起動していません。再開するとワークスペースを利用できます。',
    runtimeStatusFailed: 'サービスの状態を確認できませんでした。',
    runtimeStart: 'Swarmを起動',
    runtimeStarting: '起動中…',
    runtimeStop: 'Swarmを完全停止',
    runtimeStopConfirmTitle: 'Swarmを完全停止しますか？',
    runtimeStopConfirmText: 'バックグラウンドサービスを停止し、新しいCLI操作を拒否します。このウィンドウを閉じるだけではサービスは停止しません。',
    runtimeStopConfirmAction: '完全停止する',
    runtimeStopCancel: '稼働を続ける',
    runtimeStopFailed: '完全停止を確認できませんでした。状態を確認してください。',
    runtimeStartFailed: 'バックグラウンドサービスを起動できませんでした。状態を確認して再試行してください。',
    runtimeRefreshFailed: 'サービスは起動しましたが、ワークスペースを更新できませんでした。状態を確認して接続を再試行してください。',
    runtimeRetryStart: 'Swarmを起動',
    runtimeStoppedBanner: 'Swarmは完全停止中です。サービスを起動するまでワークスペース操作を一時停止します。',
    runtimeCloseNote: 'デスクトップのウィンドウを閉じてもバックグラウンドサービスは稼働します。',
    startupTitle: 'Windowsサインイン時の起動',
    startupToggle: 'Windowsへのサインイン時にSwarmを起動',
    startupStatusLoading: 'Windowsの起動登録を確認しています…',
    startupDefault: '初期設定（有効）を使用しています',
    startupEnabled: 'Windowsサインイン時に起動します',
    startupDisabled: 'Windowsサインイン時に起動しません',
    startupRegistrationMissing: '起動設定は有効ですが、Windowsへの登録がありません。',
    startupRegistrationUnexpected: '起動設定は無効ですが、Windowsへの登録が残っています。',
    startupStatusFailed: 'Windowsの起動設定を確認できませんでした。',
    startupApply: '起動設定を再適用',
    startupApplyFailed: 'Windowsの起動登録を更新できませんでした。表示された設定を確認して再試行してください。',
    startupMismatchFailed: 'Windowsの起動登録が希望の設定と一致しません。再適用して修復してください。',
  },
} as const

function faultDetails(error: unknown): GuiFault | null {
  if (typeof error === 'object' && error !== null && 'code' in error && 'exit_code' in error && 'message' in error) {
    const candidate = error as GuiFault
    if (typeof candidate.code === 'string' && typeof candidate.exit_code === 'number' && typeof candidate.message === 'string') {
      return candidate
    }
  }
  return null
}

function isStartupStatus(value: unknown): value is StartupStatus {
  return typeof value === 'object' && value !== null
    && 'enabled' in value && typeof value.enabled === 'boolean'
    && 'registered' in value && typeof value.registered === 'boolean'
    && 'configured' in value && typeof value.configured === 'boolean'
}

export default function App() {
  const availableUpdate = useStartupUpdate()
  const updateNotes = useUpdateNotes()
  useEffect(() => {
    let active = true
    let unlisten: () => void = () => undefined
    void installNativeClosePreferenceListener().then((stop) => {
      if (active) unlisten = stop
      else stop()
    }).catch(() => undefined)
    return () => { active = false; unlisten() }
  }, [])
  const [snapshot, setSnapshot] = useState<GuiSnapshot | null>(null)
  const [view, setView] = useState<ViewState>('loading')
  const [name, setName] = useState('')
  const [password, setPassword] = useState('')
  const [busy, setBusy] = useState(false)
  const [errorText, setErrorText] = useState('')
  const [fault, setFault] = useState<GuiFault | null>(null)
  const [formError, setFormError] = useState('')
  const [recoveryOpen, setRecoveryOpen] = useState(false)
  const [recoveryName, setRecoveryName] = useState('')
  const [recoveryCodeInput, setRecoveryCodeInput] = useState('')
  const [recoveryPassword, setRecoveryPassword] = useState('')
  const [recoveryError, setRecoveryError] = useState('')
  const [pendingDiscardOpen, setPendingDiscardOpen] = useState(false)
  const [pendingDiscardError, setPendingDiscardError] = useState('')
  const [settingsOpen, setSettingsOpen] = useState(false)
  const [settingsTab, setSettingsTab] = useState<'basic' | 'account' | 'projects' | 'permissions' | 'notifications' | 'backup' | 'retention' | 'runtime' | 'bug-reports'>('basic')
  const settingsPanels = useRef<HTMLDivElement | null>(null)
  const [runtimeStatus, setRuntimeStatus] = useState<RuntimeStatus | null>(null)
  const [runtimeStatusLoading, setRuntimeStatusLoading] = useState(false)
  const [runtimeTransition, setRuntimeTransition] = useState<'starting' | 'stopping' | null>(null)
  const [runtimeError, setRuntimeError] = useState('')
  const [runtimeStopConfirm, setRuntimeStopConfirm] = useState(false)
  const [startupStatus, setStartupStatus] = useState<StartupStatus | null>(null)
  const [startupStatusLoading, setStartupStatusLoading] = useState(false)
  const [startupError, setStartupError] = useState('')
  const [startupRetryEnabled, setStartupRetryEnabled] = useState<boolean | null>(null)
  const [authorizationRefreshToken, setAuthorizationRefreshToken] = useState(0)
  const [accountManagementBusy, setAccountManagementBusy] = useState(false)
  const [restoreOutcome, setRestoreOutcome] = useState<import('./bridge').RestoreOutcome | null>(null)
  const accountManagementBusyRef = useRef(false)
  const recoveryCodeInputRef = useRef<HTMLInputElement | null>(null)
  const recoveryPasswordInputRef = useRef<HTMLInputElement | null>(null)
  const settingsDialog = useRef<HTMLElement | null>(null)
  const settingsOpener = useRef<HTMLButtonElement | null>(null)
  const settingsWasOpen = useRef(false)
  const running = useRef(false)
  const runtimeStatusGeneration = useRef(0)
  const startupStatusGeneration = useRef(0)
  const currentAccountId = view === 'ready' && snapshot?.account && !snapshot.pendingIssue ? snapshot.account.id : null
  const display = useDisplaySettings(currentAccountId)
  const { language, theme } = display.settings
  const nativeTheme = useNativeWindowTheme(theme, display.ready)
  const setLanguage = (value: Language) => { void display.change({ language: value }) }
  const setTheme = (value: Theme) => { void display.change({ theme: value }) }
  const currentAccountIdRef = useRef<string | null>(currentAccountId)
  useEffect(() => { setSettingsTab('basic') }, [currentAccountId])
  currentAccountIdRef.current = currentAccountId
  const languageRef = useRef(language)
  languageRef.current = language
  const t = copy[language]

  const applySnapshot = useCallback((next: GuiSnapshot) => {
    setSnapshot(next)
    setView('ready')
    setErrorText('')
    setFault(null)
    setRecoveryOpen(false)
    setRecoveryName('')
    setRecoveryCodeInput('')
    setRecoveryPassword('')
    setRecoveryError('')
    if (recoveryCodeInputRef.current) recoveryCodeInputRef.current.value = ''
    if (recoveryPasswordInputRef.current) recoveryPasswordInputRef.current.value = ''
    setPendingDiscardOpen(false)
    setPendingDiscardError('')
  }, [])

  const run = useCallback(async (operation: () => Promise<GuiSnapshot>, recoverAuthenticationFault = false) => {
    if (running.current || accountManagementBusyRef.current) return
    running.current = true
    setBusy(true)
    setErrorText('')
    setFault(null)
    try {
      applySnapshot(await operation())
    } catch (error) {
      const details = faultDetails(error)
      const recoverableAuthenticationExitCode = details && details.exit_code >= 2 && details.exit_code <= 6
      if (recoverAuthenticationFault && details && recoverableAuthenticationExitCode) {
        setErrorText(details.message)
        setFault(details)
        try {
          const current = await getSnapshot()
          setSnapshot(current)
          setView('ready')
        } catch (refreshError) {
          const refreshFault = faultDetails(refreshError)
          setFault(refreshFault)
          setErrorText(refreshFault?.message || copy[languageRef.current].requestFailed)
          setView('connection-failed')
        }
        return
      }
      setFault(details)
      setErrorText(details?.message || copy[languageRef.current].requestFailed)
      setView('connection-failed')
    } finally {
      running.current = false
      setBusy(false)
    }
  }, [applySnapshot])

  useEffect(() => {
    if (!('__TAURI_INTERNALS__' in window)) {
      setView('desktop-required')
      return
    }
    void run(getSnapshot)
  }, [run])

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (running.current || !snapshot) return
    const mode: AuthMode = snapshot.setupRequired ? 'setup' : 'login'
    const submittedName = name.trim()
    const submittedPassword = password
    setPassword('')
    setFormError('')
    if (!submittedName) {
      setFormError(t.invalidName)
      return
    }
    if (!submittedPassword && mode === 'setup') {
      setFormError(t.invalidPassword)
      return
    }
    await run(() => authenticate(mode, submittedName, submittedPassword), true)
  }

  async function skipInitialPassword() {
    if (running.current || !snapshot?.setupRequired) return
    const submittedName = name.trim()
    setPassword('')
    setFormError('')
    if (!submittedName) { setFormError(t.invalidName); return }
    await run(() => authenticate('setup', submittedName, ''), true)
  }

  function clearRecoveryForm() {
    if (recoveryCodeInputRef.current) recoveryCodeInputRef.current.value = ''
    if (recoveryPasswordInputRef.current) recoveryPasswordInputRef.current.value = ''
    setRecoveryName('')
    setRecoveryCodeInput('')
    setRecoveryPassword('')
    setRecoveryError('')
  }

  async function submitRecovery(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (running.current || accountManagementBusyRef.current || busy || !snapshot || snapshot.setupRequired) return
    const submittedName = recoveryName.trim()
    const submittedCode = recoveryCodeInput
    const submittedPassword = recoveryPassword
    if (recoveryCodeInputRef.current) recoveryCodeInputRef.current.value = ''
    if (recoveryPasswordInputRef.current) recoveryPasswordInputRef.current.value = ''
    setRecoveryCodeInput('')
    setRecoveryPassword('')
    setRecoveryError('')
    if (!submittedName || !submittedCode || !submittedPassword) {
      setRecoveryError(t.recoveryFailed)
      return
    }
    running.current = true
    setBusy(true)
    try {
      applySnapshot(await recoverAccount(submittedName, submittedCode, submittedPassword))
    } catch {
      try {
        const refreshed = await getSnapshot()
        if (refreshed.pendingIssue || refreshed.account) {
          applySnapshot(refreshed)
        } else {
          setSnapshot(refreshed)
          setView('ready')
          setRecoveryError(t.recoveryFailed)
        }
      } catch {
        setSnapshot(null)
        setFault(null)
        setErrorText(t.requestFailed)
        setView('connection-failed')
      }
    } finally {
      running.current = false
      setBusy(false)
    }
  }

  async function confirmDiscardPending() {
    if (running.current || accountManagementBusyRef.current || busy) return
    setPendingDiscardOpen(false)
    setPendingDiscardError('')
    running.current = true
    setBusy(true)
    try {
      applySnapshot(await cancelPending())
    } catch {
      try {
        const refreshed = await getSnapshot()
        if (refreshed.pendingIssue) {
          setSnapshot(refreshed)
          setView('ready')
          setPendingDiscardError(t.discardPendingError)
        } else {
          applySnapshot(refreshed)
        }
      } catch {
        setSnapshot(null)
        setFault(null)
        setErrorText(t.requestFailed)
        setView('connection-failed')
      }
    } finally {
      running.current = false
      setBusy(false)
    }
  }

  const refresh = () => run(getSnapshot)
  const invalidatePrivateSettings = useCallback(() => {
    setSettingsOpen(false)
    setAuthorizationRefreshToken(value => value + 1)
    void run(getSnapshot)
  }, [run])
  const retry = () => run(retryPending)
  const signOut = () => run(logout)
  const refreshAccountSnapshot = useCallback(async () => {
    try {
      applySnapshot(await getSnapshot())
    } catch (error) {
      const details = faultDetails(error)
      setSnapshot(null)
      setFault(details)
      setErrorText(details?.message || copy[languageRef.current].requestFailed)
      setView('connection-failed')
      throw error
    }
  }, [applySnapshot])
  const setAccountManagementBusyState = useCallback((nextBusy: boolean) => {
    if (!nextBusy && running.current) return
    accountManagementBusyRef.current = nextBusy
    setAccountManagementBusy(nextBusy)
  }, [])
  const beginAccountAccessOperation = useCallback(() => {
    if (running.current || accountManagementBusyRef.current) return false
    running.current = true
    setBusy(true)
    accountManagementBusyRef.current = true
    setAccountManagementBusy(true)
    return true
  }, [])
  const endAccountAccessOperation = useCallback(() => {
    running.current = false
    setBusy(false)
    accountManagementBusyRef.current = false
    setAccountManagementBusy(false)
  }, [])
  const failClosedAccountState = useCallback(() => {
    setSnapshot(null)
    setFault(null)
    setErrorText(copy[languageRef.current].requestFailed)
    setView('connection-failed')
  }, [])
  const refreshAfterAuthorizationChange = useCallback(async () => {
    await refreshAccountSnapshot()
    setAuthorizationRefreshToken((current) => current + 1)
  }, [refreshAccountSnapshot])
  const restoreFinished = useCallback((outcome: import('./bridge').RestoreOutcome) => {
    setRestoreOutcome(outcome)
    setSettingsOpen(false)
    void refreshAccountSnapshot().catch(() => { /* Existing fail-closed state presents reconnect. */ })
  }, [refreshAccountSnapshot])
  const readRuntimeStatus = useCallback(async (options: { isActive?: () => boolean; preserveError?: boolean } = {}) => {
    const generation = ++runtimeStatusGeneration.current
    const isCurrent = () => generation === runtimeStatusGeneration.current && (options.isActive?.() ?? true)
    setRuntimeStatusLoading(true)
    try {
      const next = await guiRuntimeStatus()
      if (!isCurrent()) return null
      setRuntimeStatus(next)
      if (!options.preserveError) setRuntimeError('')
      return next
    } catch (error) {
      if (isCurrent() && !options.preserveError) {
        const details = faultDetails(error)
        setRuntimeError(details?.message || copy[languageRef.current].runtimeStatusFailed)
      }
      return null
    } finally {
      if (isCurrent()) setRuntimeStatusLoading(false)
    }
  }, [])

  useEffect(() => {
    if (!currentAccountId) return
    clearOperationStatuses()
    let active = true
    void readRuntimeStatus({ isActive: () => active })
    return () => { active = false }
  }, [currentAccountId, readRuntimeStatus])

  const readStartupStatus = useCallback(async (options: { isActive?: () => boolean; preserveError?: boolean } = {}) => {
    const generation = ++startupStatusGeneration.current
    const accountId = currentAccountIdRef.current
    const isCurrent = () => generation === startupStatusGeneration.current
      && accountId === currentAccountIdRef.current
      && (options.isActive?.() ?? true)
    setStartupStatusLoading(true)
    try {
      const next: unknown = await guiStartupStatus()
      if (!isStartupStatus(next)) throw new Error('Invalid startup status response')
      if (!isCurrent()) return null
      setStartupStatus(next)
      if (!options.preserveError) setStartupError('')
      return next
    } catch {
      if (isCurrent()) {
        setStartupStatus(null)
        if (!options.preserveError) setStartupError(copy[languageRef.current].startupStatusFailed)
      }
      return null
    } finally {
      if (isCurrent()) setStartupStatusLoading(false)
    }
  }, [])

  async function updateStartupSetting(enabled: boolean) {
    if (!beginAccountAccessOperation()) return
    const generation = ++startupStatusGeneration.current
    const accountId = currentAccountIdRef.current
    const isCurrent = () => generation === startupStatusGeneration.current && accountId === currentAccountIdRef.current
    setStartupStatusLoading(false)
    setStartupRetryEnabled(enabled)
    setStartupError('')
    try {
      const next: unknown = await guiStartupSet(enabled)
      if (!isStartupStatus(next)) throw new Error('Invalid startup setting response')
      if (!isCurrent()) return
      setStartupStatus(next)
      if (next.enabled !== enabled || next.registered !== enabled) {
        setStartupError(copy[languageRef.current].startupMismatchFailed)
      } else {
        setStartupRetryEnabled(null)
        setStartupError('')
      }
    } catch {
      if (!isCurrent()) return
      setStartupError(copy[languageRef.current].startupApplyFailed)
      await readStartupStatus({ preserveError: true })
    } finally {
      endAccountAccessOperation()
    }
  }

  async function startRuntime() {
    if (!beginAccountAccessOperation()) return
    setRuntimeTransition('starting')
    runtimeStatusGeneration.current += 1
    setRuntimeStatusLoading(false)
    setRuntimeError('')
    let started = false
    try {
      const status = await guiRuntimeStart()
      runtimeStatusGeneration.current += 1
      setRuntimeStatus(status)
      started = true
      applySnapshot(await getSnapshot())
      setRuntimeError('')
      setRuntimeStopConfirm(false)
    } catch (error) {
      const details = faultDetails(error)
      const message = started
        ? copy[languageRef.current].runtimeRefreshFailed
        : details?.message || copy[languageRef.current].runtimeStartFailed
      if (started) {
        setSnapshot(null)
        setFault(null)
        setErrorText(message)
        setView('connection-failed')
      }
      setRuntimeError(message)
      await readRuntimeStatus({ preserveError: true })
    } finally {
      setRuntimeTransition(null)
      endAccountAccessOperation()
    }
  }

  async function stopRuntime() {
    if (!beginAccountAccessOperation()) return
    setRuntimeTransition('stopping')
    runtimeStatusGeneration.current += 1
    setRuntimeStatusLoading(false)
    setRuntimeError('')
    try {
      const next = await guiRuntimeStop()
      runtimeStatusGeneration.current += 1
      setRuntimeStatus(next)
      setRuntimeStopConfirm(false)
    } catch (error) {
      const current = await readRuntimeStatus()
      if (current?.state === 'stopped') {
        setRuntimeStopConfirm(false)
      } else {
        const details = faultDetails(error)
        setRuntimeError(details?.message || copy[languageRef.current].runtimeStopFailed)
      }
    } finally {
      setRuntimeTransition(null)
      endAccountAccessOperation()
    }
  }
  const openSettings = (opener: HTMLButtonElement) => {
    settingsOpener.current = opener
    setSettingsTab('basic')
    setSettingsOpen(true)
  }
  const closeSettings = useCallback(() => {
    if (!accountManagementBusyRef.current) setSettingsOpen(false)
  }, [])

  useEffect(() => {
    if (!settingsOpen) return
    let active = true
    void readRuntimeStatus({ isActive: () => active })
    void readStartupStatus({ isActive: () => active })
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') { closeSettings(); return }
      if (event.key !== 'Tab') return
      const focusable = [...(settingsDialog.current?.querySelectorAll<HTMLElement>('button:not(:disabled), [href], input:not(:disabled), textarea:not(:disabled), select:not(:disabled), [tabindex]:not([tabindex="-1"])') || [])].filter((element) => !element.closest('[hidden]') && element.tabIndex >= 0)
      if (!focusable?.length) { event.preventDefault(); return }
      const first = focusable[0]
      const last = focusable[focusable.length - 1]
      if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus() }
      else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus() }
    }
    settingsWasOpen.current = true
    settingsDialog.current?.querySelector<HTMLButtonElement>('[role="tab"][aria-selected="true"]')?.focus()
    document.addEventListener('keydown', onKeyDown)
    return () => {
      active = false
      runtimeStatusGeneration.current += 1
      startupStatusGeneration.current += 1
      document.removeEventListener('keydown', onKeyDown)
    }
  }, [settingsOpen, currentAccountId, closeSettings, readRuntimeStatus, readStartupStatus])

  useEffect(() => {
    if (settingsOpen || !settingsWasOpen.current) return
    settingsWasOpen.current = false
    if (settingsOpener.current?.isConnected) settingsOpener.current.focus()
    settingsOpener.current = null
  }, [settingsOpen])

  useEffect(() => {
    if (settingsOpen && !(view === 'ready' && snapshot?.account && !snapshot.pendingIssue)) setSettingsOpen(false)
  }, [settingsOpen, snapshot?.account, snapshot?.pendingIssue, view])

  return (
    <main className={`app-shell${view === 'ready' && snapshot?.account && !snapshot.pendingIssue ? ' has-workspace-app' : ''}`} data-theme={theme} data-restriction-state={busy ? 'busy' : runtimeStatus && runtimeStatus.state !== 'running' ? 'runtime' : undefined}>
      <div id="app-content" className="app-content" inert={settingsOpen || (runtimeStatus !== null && runtimeStatus.state !== 'running') || undefined} aria-hidden={settingsOpen || (runtimeStatus !== null && runtimeStatus.state !== 'running') || undefined}>
      {!(view === 'ready' && snapshot?.account && !snapshot.pendingIssue) && <header className="topbar">
        <a className="brand" href="#home" aria-label="Swarm home">
          <img className="brand-icon" src="/icon.png" alt="" />
          <span>Swarm</span>
        </a>
        {!(view === 'ready' && snapshot?.account && !snapshot.pendingIssue) && <div className="topbar-controls">
          <div className="setting-control">
            <button className="text-button" type="button" aria-label={t.languageLabel} onClick={() => setLanguage(language === 'en' ? 'ja' : 'en')} disabled={!display.ready || display.saving}>
              {t.language}
            </button>
          </div>
          <div className="setting-control">
            <button className="theme-button" type="button" aria-label={t.appearanceLabel} onClick={() => setTheme(theme === 'light' ? 'dark' : 'light')} disabled={!display.ready || display.saving}>
              <span className="theme-dot" aria-hidden="true" />
              {theme === 'light' ? t.theme : t.themeLight}
            </button>
          </div>
        </div>}
      </header>}
      {display.failed && !currentAccountId && <p className="workspace-error" role="alert">{language === 'ja' ? 'Windowsの表示設定を確認できませんでした。' : 'Could not read Windows display defaults.'}<button className="text-button" type="button" onClick={() => void display.refresh()}>{language === 'ja' ? '再試行' : 'Retry'}</button></p>}
      {nativeTheme.failed && !settingsOpen && <p className="workspace-error" role="alert">{language === 'ja' ? 'タイトルバーの表示を変更できませんでした。' : 'Could not update the title bar appearance.'}<button className="text-button" type="button" onClick={nativeTheme.retry}>{language === 'ja' ? '再試行' : 'Retry'}</button></p>}
      {restoreOutcome && <p role={restoreOutcome === 'uncertain' ? 'alert' : 'status'} className="restore-outcome">{restoreOutcome === 'applied' ? language === 'ja' ? '復元を適用しました。ユーザーは再ログインし、AIは新しい資格を取得してください。' : 'Restore applied. Sign in again. AI must obtain new credentials.' : restoreOutcome === 'rolled_back' ? language === 'ja' ? '復元前の状態へ巻き戻しました。再ログインしてください。' : 'Rolled back to the state before restoration. Sign in again.' : language === 'ja' ? '復元結果が不確定です。自動再送せず、再ログイン後に復元準備の状態を確認してください。復旧要求が出た場合はruntimeを再起動してください。' : 'Restore result is uncertain. Sign in again and check restore preparation without automatically resending. Restart the runtime if recovery is required.'}</p>}

      <div className={`content-wrap${view === 'ready' && snapshot?.account && !snapshot.pendingIssue ? ' has-workspace' : ''}`} id="home">
        <section className="welcome-block" aria-labelledby="welcome-title">
          <p className="eyebrow">{t.eyebrow}</p>
          <h1 id="welcome-title">{t.tagline}</h1>
        </section>

        {view === 'loading' && <section className="panel status-panel" aria-live="polite"><span className="spinner" aria-hidden="true" />{t.loading}</section>}

        {view === 'desktop-required' && (
          <section className="panel notice-panel" aria-labelledby="desktop-title">
            <div className="notice-icon" aria-hidden="true">↗</div>
            <div><h2 id="desktop-title">{t.desktopRequiredTitle}</h2><p>{t.desktopRequiredBody}</p></div>
          </section>
        )}

        {view === 'connection-failed' && (
          <section className="panel notice-panel" aria-labelledby="connection-title">
            <div className="notice-icon error-icon" aria-hidden="true">!</div>
            <div className="notice-copy">
              <h2 id="connection-title">{t.connectionTitle}</h2>
              <p>{t.connectionBody}</p>
              {errorText && <div className="fault-detail" role="alert">{errorText}{fault && <span className="fault-code">{fault.code} · {fault.exit_code}</span>}</div>}
              {runtimeError && <div className="fault-detail" data-testid="runtime-error" role="alert">{runtimeError}</div>}
              <button className="button button-primary" type="button" onClick={refresh} disabled={busy}>
                {busy ? t.loading : t.retry}
              </button>
              <button className="button button-secondary" type="button" data-testid="connection-runtime-start" onClick={() => void startRuntime()} disabled={busy || accountManagementBusy}>
                {busy ? t.runtimeStarting : t.runtimeRetryStart}
              </button>
            </div>
          </section>
        )}

        {view === 'ready' && snapshot?.pendingIssue && (
          <section className="panel notice-panel" aria-labelledby="pending-title">
            <div className="notice-icon pending-icon" aria-hidden="true">↻</div>
            <div className="notice-copy">
              <h2 id="pending-title">{t.pendingTitle}</h2>
              <p>{t.pendingBody}</p>
              {errorText && <div className="fault-detail" role="alert">{errorText}</div>}
              <button className="button button-primary" type="button" onClick={retry} disabled={busy}>
                {busy ? t.loading : t.retryPending}
              </button>
              {pendingDiscardError && <p className="account-access-error" data-testid="pending-discard-error" role="alert">{pendingDiscardError}</p>}
              {!pendingDiscardOpen
                ? <button className="text-button pending-discard-toggle" type="button" data-testid="pending-discard-toggle" onClick={() => { setPendingDiscardError(''); setPendingDiscardOpen(true) }} disabled={busy}>{t.discardPending}</button>
                : <div className="pending-discard-confirmation" role="group" aria-label={t.discardPending}>
                    <p>{t.discardPendingExplain}</p>
                    <div className="account-access-actions"><button className="button button-secondary" type="button" data-testid="pending-discard-cancel" onClick={() => setPendingDiscardOpen(false)} disabled={busy}>{t.cancel}</button><button className="button button-primary" type="button" data-testid="pending-discard-confirm" onClick={() => void confirmDiscardPending()} disabled={busy}>{busy ? t.loading : t.discardPendingConfirm}</button></div>
                  </div>}
            </div>
          </section>
        )}

        {view === 'ready' && snapshot && !snapshot.pendingIssue && !snapshot.account && (
          <section className="panel auth-panel" aria-labelledby="auth-title">
            <div className="panel-heading">
              <div className="step-mark" aria-hidden="true">{snapshot.setupRequired ? '01' : '↗'}</div>
              <div>
                <p className="panel-kicker">{snapshot.setupRequired ? 'FIRST TIME HERE' : 'ACCOUNT ACCESS'}</p>
                <h2 id="auth-title">{snapshot.setupRequired ? t.setupTitle : t.loginTitle}</h2>
                <p className="panel-description">{snapshot.setupRequired ? t.setupBody : t.loginBody}</p>
              </div>
            </div>
            <form className="auth-form" onSubmit={submit}>
              <label className="field">
                <span>{t.name}</span>
                <AccountNameInput history={snapshot.accountNameHistory} value={name} onChange={(event) => setName(event.target.value)} placeholder={t.namePlaceholder} disabled={busy} />
              </label>
              <label className="field">
                <span>{t.password}</span>
                <input type="password" autoComplete={snapshot.setupRequired ? 'new-password' : 'current-password'} value={password} onChange={(event) => setPassword(event.target.value)} placeholder={t.passwordPlaceholder} disabled={busy} />
              </label>
              {formError && <p className="form-error" role="alert">{formError}</p>}
              <p className="account-access-hint">{snapshot.setupRequired ? t.passwordLater : t.unlockedLoginHint}</p>
              {errorText && <p className="form-error" role="alert">{errorText}{fault && <span className="fault-code">{fault.code} · {fault.exit_code}</span>}</p>}
              <button className="button button-primary submit-button" type="submit" disabled={busy}>
                {busy ? <><span className="spinner spinner-small" aria-hidden="true" />{t.loading}</> : snapshot.setupRequired ? t.setupAction : t.loginAction}
                {!busy && <span aria-hidden="true">→</span>}
              </button>
              {snapshot.setupRequired && <button className="button button-secondary" type="button" data-testid="setup-skip-password" onClick={() => void skipInitialPassword()} disabled={busy}>{t.skipPassword}</button>}
            </form>
            {!snapshot.setupRequired && <section className="admin-recovery" aria-label={t.recoverAdmin}>
              {!recoveryOpen
                ? <button className="text-button admin-recovery-toggle" type="button" data-testid="admin-recovery-toggle" onClick={() => { clearRecoveryForm(); setRecoveryOpen(true) }} disabled={busy}>{t.recoverAdmin}</button>
                : <form className="admin-recovery-form" data-testid="admin-recovery-form" onSubmit={submitRecovery}>
                    <h3>{t.recoverAdmin}</h3>
                    <p className="account-access-hint">{t.recoveryEffect}</p>
                    <label className="field"><span>{t.recoveryName}</span><AccountNameInput history={snapshot.accountNameHistory} data-testid="admin-recovery-name" value={recoveryName} onChange={(event) => setRecoveryName(event.target.value)} disabled={busy} /></label>
                    <label className="field"><span>{t.recoveryCode}</span><input ref={recoveryCodeInputRef} data-testid="admin-recovery-code" type="password" autoComplete="off" value={recoveryCodeInput} onChange={(event) => setRecoveryCodeInput(event.target.value)} disabled={busy} /></label>
                    <label className="field"><span>{t.recoveryPassword}</span><input ref={recoveryPasswordInputRef} data-testid="admin-recovery-password" type="password" autoComplete="new-password" value={recoveryPassword} onChange={(event) => setRecoveryPassword(event.target.value)} disabled={busy} /></label>
                    {recoveryError && <p className="form-error" data-testid="admin-recovery-error" role="alert">{recoveryError}</p>}
                    <div className="account-access-actions"><button className="button button-secondary" type="button" data-testid="admin-recovery-cancel" onClick={() => { setRecoveryOpen(false); clearRecoveryForm() }} disabled={busy}>{t.recoveryCancel}</button><button className="button button-primary" type="submit" data-testid="admin-recovery-submit" disabled={busy}>{busy ? t.loading : t.recoverySubmit}</button></div>
                  </form>}
            </section>}
            {!snapshot.setupRequired && <GoogleRecoveryForm language={language} busy={busy} onRecovered={operation=>run(operation,true)} />}
          </section>
        )}

        {view === 'ready' && snapshot?.account && !snapshot.pendingIssue && (
          <section className="dashboard-stack">
            <Workspace key={snapshot.account.id} accountId={snapshot.account.id} language={language} onLogout={signOut} onSettings={openSettings} onSessionExpired={refresh} busy={busy} refreshToken={authorizationRefreshToken} serviceState={runtimeTransition || (runtimeStatusLoading ? 'checking' : runtimeStatus?.state || 'unknown')} serviceError={runtimeError} serviceBusy={runtimeStatusLoading} onServiceRefresh={() => void readRuntimeStatus()} availableUpdate={availableUpdate} updateNotes={updateNotes} />
          </section>
        )}

        <footer className="page-footer">
          <span className="footer-pulse" aria-hidden="true" />
          <span>{t.localSettings}</span>
        </footer>
      </div>
      </div>
      {runtimeStatus !== null && runtimeStatus.state !== 'running' && <section className="runtime-stopped-banner" data-testid="runtime-stopped-banner" role="status">
        <p>{runtimeStatus.state === 'stopped' ? t.runtimeStoppedBanner : t.runtimeNotRunningBanner}</p>
        <button className="button button-primary" type="button" data-testid="runtime-banner-start" onClick={() => void startRuntime()} disabled={busy || accountManagementBusy}>{busy ? t.runtimeStarting : t.runtimeStart}</button>
      </section>}
      {settingsOpen && view === 'ready' && snapshot?.account && !snapshot.pendingIssue && <div className="settings-backdrop">
        <section className="settings-dialog" role="dialog" aria-modal="true" aria-label={t.settings} ref={settingsDialog}>
          <Tabs id="settings" label={language === 'ja' ? '設定カテゴリ' : 'Settings categories'} value={settingsTab} disabled={busy || accountManagementBusy || display.saving}
            items={[
              { value: 'basic', label: language === 'ja' ? '基本設定' : 'Basic settings' },
              { value: 'bug-reports', label: language === 'ja' ? '不具合報告' : 'Bug reports' },
              { value: 'account', label: language === 'ja' ? 'アカウント' : 'Account' },
              ...(snapshot.account.is_admin ? [{ value: 'projects' as const, label: language === 'ja' ? 'プロジェクト' : 'Projects' }] : []),
              { value: 'permissions', label: language === 'ja' ? '権限・ルール' : 'Permissions & rules' },
              { value: 'notifications', label: language === 'ja' ? '通知' : 'Notifications' },
              { value: 'retention', label: language === 'ja' ? 'データ保持' : 'Data retention' },
              ...(snapshot.account.is_admin ? [{ value: 'backup' as const, label: language === 'ja' ? 'バックアップ' : 'Backup' }] : []),
              { value: 'runtime', label: language === 'ja' ? '起動・常駐' : 'Startup & runtime' },
            ]} onChange={(next) => { setSettingsTab(next); if (settingsPanels.current) settingsPanels.current.scrollTop = 0 }} />
          <div className="settings-panels" ref={settingsPanels}>
          <section className="settings-panel" role="tabpanel" id="settings-panel-basic" aria-labelledby="settings-tab-basic" hidden={settingsTab !== 'basic'}>
          <section className="settings-preferences" aria-label={language === 'ja' ? '基本設定' : 'Basic settings'}>
            <div className="settings-preference"><span>{t.languageLabel}</span><button className="text-button" type="button" aria-label={t.languageLabel} onClick={() => setLanguage(language === 'en' ? 'ja' : 'en')} disabled={!display.ready || display.saving || busy || runtimeStatus?.state !== 'running'}>{t.language}</button></div>
            <div className="settings-preference"><span>{t.appearanceLabel}</span><button className="theme-button" type="button" aria-label={t.appearanceLabel} onClick={() => setTheme(theme === 'light' ? 'dark' : 'light')} disabled={!display.ready || display.saving || busy || runtimeStatus?.state !== 'running'}><span className="theme-dot" aria-hidden="true" />{theme === 'light' ? t.theme : t.themeLight}</button></div>
            {display.saving && <p role="status">{language === 'ja' ? '表示設定を保存中…' : 'Saving display settings…'}</p>}
          </section>
          <UpdateSettings language={language} />
          {display.failed && <p className="workspace-error" role="alert">{language === 'ja' ? '表示設定を確認できませんでした。現在の設定を確認してから変更してください。' : 'Could not confirm display settings. Check current settings before changing them.'}<button className="text-button" type="button" onClick={() => void display.refresh()} disabled={display.saving}>{language === 'ja' ? '表示設定を確認' : 'Check display settings'}</button></p>}
          {nativeTheme.failed && <p className="workspace-error" role="alert">{language === 'ja' ? 'タイトルバーの表示を変更できませんでした。' : 'Could not update the title bar appearance.'}<button className="text-button" type="button" onClick={nativeTheme.retry}>{language === 'ja' ? '再試行' : 'Retry'}</button></p>}
          </section>
          <section className="settings-panel" role="tabpanel" id="settings-panel-bug-reports" aria-labelledby="settings-tab-bug-reports" hidden={settingsTab !== 'bug-reports'}>
            <BugReportsPanel language={language} accountId={snapshot.account.id} isAdmin={snapshot.account.is_admin}/>
          </section>
          <section className="settings-panel" role="tabpanel" id="settings-panel-account" aria-labelledby="settings-tab-account" hidden={settingsTab !== 'account'}>
          <section className="settings-account" aria-labelledby="settings-account-title">
            <h3 id="settings-account-title">{t.accountTitle}</h3>
            <dl>
              <div><dt>{t.name}</dt><dd id="settings-account-name">{snapshot.account.name}</dd></div>
              <div><dt>{t.accountStatus}</dt><dd id="settings-account-status">{snapshot.account.disabled ? t.disabled : snapshot.account.is_admin ? t.admin : t.member}</dd></div>
              <div><dt>{t.accountId}</dt><dd id="settings-account-id">{snapshot.account.id}</dd></div>
              <div><dt>{t.roles}</dt><dd id="settings-account-roles">{snapshot.account.role_ids.length ? snapshot.account.role_ids.join(', ') : t.noRoles}</dd></div>
            </dl>
          </section>
          <AccountManagement
            key={`management:${snapshot.account.id}:${authorizationRefreshToken}`}
            language={language}
            currentAccountId={snapshot.account.id}
            busy={busy || accountManagementBusy}
            onAccountChanged={refreshAccountSnapshot}
            onAccessLost={invalidatePrivateSettings}
            onBusyChange={setAccountManagementBusyState}
          />
          <AccountAccess
            key={`access:${snapshot.account.id}:${authorizationRefreshToken}`}
            language={language}
            account={snapshot.account}
            accountNameHistory={snapshot.accountNameHistory}
            busy={busy || accountManagementBusy}
            beginOperation={beginAccountAccessOperation}
            endOperation={endAccountAccessOperation}
            onAccountChanged={applySnapshot}
            onUncertain={failClosedAccountState}
          />
          <PasswordLockPanel key={`lock:${snapshot.account.id}`} language={language} account={snapshot.account} busy={busy || accountManagementBusy} beginOperation={beginAccountAccessOperation} endOperation={endAccountAccessOperation} onAccountChanged={applySnapshot} onUncertain={failClosedAccountState} />
          <GoogleRecoverySettings key={`google:${snapshot.account.id}`} language={language} account={snapshot.account} busy={busy || accountManagementBusy} beginOperation={beginAccountAccessOperation} endOperation={endAccountAccessOperation} onAccountChanged={applySnapshot} onUncertain={failClosedAccountState} />
          </section>
          {snapshot.account.is_admin && <section className="settings-panel" role="tabpanel" id="settings-panel-projects" aria-labelledby="settings-tab-projects" hidden={settingsTab !== 'projects'}>
            {settingsTab === 'projects' && <ProjectSettingsPanel key={snapshot.account.id} accountId={snapshot.account.id} language={language} disabled={busy || runtimeStatus?.state !== 'running'} beginOperation={beginAccountAccessOperation} endOperation={endAccountAccessOperation} />}
          </section>}
          <section className="settings-panel" role="tabpanel" id="settings-panel-permissions" aria-labelledby="settings-tab-permissions" hidden={settingsTab !== 'permissions'}>
          <RulesAiGuide language={language} />
          <AuthorizationManagement
            language={language}
            currentAccountId={snapshot.account.id}
            busy={busy || accountManagementBusy}
            beginOperation={beginAccountAccessOperation}
            endOperation={endAccountAccessOperation}
            onAuthorizationChanged={refreshAfterAuthorizationChange}
            onAccessLost={invalidatePrivateSettings}
          />
          <RulesEditor
            key={`rules:${snapshot.account.id}:${authorizationRefreshToken}`}
            language={language}
            currentAccountId={snapshot.account.id}
            busy={busy || accountManagementBusy}
            beginOperation={beginAccountAccessOperation}
            endOperation={endAccountAccessOperation}
            onRulesChanged={refreshAfterAuthorizationChange}
            onAccessLost={invalidatePrivateSettings}
          />
          </section>
          <section className="settings-panel" role="tabpanel" id="settings-panel-runtime" aria-labelledby="settings-tab-runtime" hidden={settingsTab !== 'runtime'}>
          <section className="runtime-controls" aria-labelledby="runtime-controls-title" data-testid="runtime-controls">
            <div className="runtime-controls-heading">
              <h3 id="runtime-controls-title">{t.runtimeTitle}</h3>
              <span className={`runtime-state runtime-state-${runtimeStatusLoading ? 'unknown' : runtimeStatus?.state || 'unknown'}`} data-testid="runtime-state" role="status">
                {runtimeStatusLoading ? t.runtimeStatusLoading : runtimeStatus?.state === 'running' ? t.runtimeRunning : runtimeStatus?.state === 'stopped' ? t.runtimeStopped : runtimeStatus?.state === 'not_running' ? t.runtimeNotRunning : t.runtimeStatusFailed}
              </span>
            </div>
            {runtimeError && <p className="runtime-error" data-testid="runtime-error" role="alert">{runtimeError}</p>}
            {runtimeStatus?.state === 'running'
              ? <button className="button button-secondary" type="button" data-testid="runtime-stop-open" onClick={() => setRuntimeStopConfirm(true)} disabled={busy || runtimeStatusLoading}>{t.runtimeStop}</button>
              : <button className="button button-primary" type="button" data-testid="runtime-start" onClick={() => void startRuntime()} disabled={busy || runtimeStatusLoading || runtimeStatus === null}>{busy ? t.runtimeStarting : t.runtimeStart}</button>}
            {runtimeStopConfirm && <div className="runtime-stop-confirmation" data-testid="runtime-stop-confirmation" role="group" aria-labelledby="runtime-stop-confirm-title">
              <h4 id="runtime-stop-confirm-title">{t.runtimeStopConfirmTitle}</h4>
              <p>{t.runtimeStopConfirmText}</p>
              <div className="runtime-control-actions">
                <button className="button button-secondary" type="button" data-testid="runtime-stop-cancel" onClick={() => setRuntimeStopConfirm(false)} disabled={busy}>{t.runtimeStopCancel}</button>
                <button className="button button-danger" type="button" data-testid="runtime-stop-confirm" onClick={() => void stopRuntime()} disabled={busy}>{busy ? t.loading : t.runtimeStopConfirmAction}</button>
              </div>
            </div>}
            <button className="text-button runtime-refresh" type="button" data-testid="runtime-status-refresh" onClick={() => void readRuntimeStatus()} disabled={runtimeStatusLoading || busy}>{t.retry}</button>
            <p className="runtime-close-note">{t.runtimeCloseNote}</p>
            <section className="windows-startup-controls" aria-labelledby="windows-startup-title" data-testid="windows-startup-controls">
              <h4 id="windows-startup-title">{t.startupTitle}</h4>
              <label className="windows-startup-toggle">
                <input
                  type="checkbox"
                  data-testid="windows-startup-toggle"
                  checked={startupStatus?.enabled ?? false}
                  onChange={(event) => void updateStartupSetting(event.currentTarget.checked)}
                  disabled={busy || startupStatusLoading || startupStatus === null || runtimeStatus?.state !== 'running'}
                  aria-describedby="windows-startup-state"
                />
                <span>{t.startupToggle}</span>
              </label>
              <p className="windows-startup-state" id="windows-startup-state" data-testid="windows-startup-state" role="status">
                {startupStatusLoading
                  ? t.startupStatusLoading
                  : startupStatus === null
                    ? t.startupStatusFailed
                    : startupStatus.enabled ? t.startupEnabled : t.startupDisabled}
                {startupStatus?.configured === false && ` · ${t.startupDefault}`}
              </p>
              {startupStatus && startupStatus.enabled !== startupStatus.registered && <p className="windows-startup-warning" data-testid="windows-startup-warning" role="status">
                {startupStatus.enabled ? t.startupRegistrationMissing : t.startupRegistrationUnexpected}
              </p>}
              {startupError && <p className="runtime-error" data-testid="windows-startup-error" role="alert">{startupError}</p>}
              {(startupStatus !== null && startupStatus.enabled !== startupStatus.registered || startupRetryEnabled !== null) && <button
                className="button button-secondary windows-startup-apply"
                type="button"
                data-testid="windows-startup-apply"
                onClick={() => void updateStartupSetting(startupRetryEnabled ?? startupStatus?.enabled ?? true)}
                disabled={busy || startupStatusLoading || runtimeStatus?.state !== 'running'}
              >{t.startupApply}</button>}
              <button className="text-button runtime-refresh" type="button" data-testid="windows-startup-refresh" onClick={() => void readStartupStatus()} disabled={startupStatusLoading || busy}>{t.retry}</button>
            </section>
          </section>
          </section>
          <section className="settings-panel" role="tabpanel" id="settings-panel-notifications" aria-labelledby="settings-tab-notifications" hidden={settingsTab !== 'notifications'}>
          <NotificationSettingsPanel key={snapshot.account.id} accountId={snapshot.account.id} language={language} disabled={busy || runtimeStatus?.state !== 'running'} />
          </section>
          <section className="settings-panel" role="tabpanel" id="settings-panel-retention" aria-labelledby="settings-tab-retention" hidden={settingsTab !== 'retention'}>
            {settingsTab === 'retention' && <RetentionSettingsPanel key={snapshot.account.id} accountId={snapshot.account.id} language={language} disabled={busy || runtimeStatus?.state !== 'running'} onBusyChange={setAccountManagementBusy} />}
          </section>
          {snapshot.account.is_admin && <section className="settings-panel" role="tabpanel" id="settings-panel-backup" aria-labelledby="settings-tab-backup" hidden={settingsTab !== 'backup'}>
          <BackupSettingsPanel key={`backup-${snapshot.account.id}`} accountId={snapshot.account.id} language={language} disabled={busy || runtimeStatus?.state !== 'running'} onBusyChange={setAccountManagementBusyState} onRestoreFinished={restoreFinished} />
          </section>}
          </div>
          <footer className="settings-dialog-footer"><p className="settings-note">{t.localSettings}</p><button className="button button-secondary settings-close-button" type="button" onClick={closeSettings} disabled={accountManagementBusy}>{t.closeSettings}</button></footer>
        </section>
      </div>}
      <ChangeSavedToast key={currentAccountId || 'guest'} language={language} />
    </main>
  )
}
