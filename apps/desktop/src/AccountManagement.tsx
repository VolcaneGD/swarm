import { useCallback, useEffect, useRef, useState, type FormEvent } from 'react'
import useSettingsCapabilityGuard from './useSettingsCapabilityGuard'
import { guiAccountManage, guiCreationStatus, guiRead } from './bridge'
import type { Account, AccountManagementArgs, AccountManageOperation, CreationStatus, ManagementCapabilities } from './types'

type Language = 'en' | 'ja'
type LoadState = 'loading' | 'ready' | 'failed'
type Confirmation = { account: Account; action: 'disable' | 'enable' } | null

const copy = {
  en: {
    title: 'Account management', unavailable: 'Account management is not available to this account.',
    loading: 'Loading account management…', loadFailed: 'Could not load account management. Refresh before trying again.', refresh: 'Refresh accounts', refreshing: 'Refreshing…',
    pending: 'A conversation creation request is pending. Finish or discard it before changing accounts.',
    noAccounts: 'No additional accounts yet.', noList: 'The account list is not available with your current permissions.',
    create: 'Add account', cancel: 'Cancel', createName: 'Account name', createPassword: 'Temporary password', createSubmit: 'Create account', creating: 'Creating…',
    disable: 'Disable', enable: 'Enable', reset: 'Reset password', resetPassword: 'New password', resetSubmit: 'Reset password', resetting: 'Resetting…',
    confirmDisable: 'Disable account', confirmEnable: 'Enable account', confirmReset: 'Confirm password reset',
    revoke: 'Disabling this account revokes its current logins. Enabling restores access for a new login; it does not restore old sessions.',
    resetRevoke: 'Resetting this password revokes the account’s current logins, including your own if you reset your account.',
    disabled: 'Disabled', active: 'Active', administrator: 'Administrator', member: 'Member',
    created: 'Account created.', updated: 'Account updated.',
    mutationUncertain: 'The account change could not be confirmed. Refresh accounts to verify the current state before another change.',
    refreshAfterChange: 'The change succeeded, but account data could not be refreshed. Refresh accounts before another change.',
    createInvalid: 'Enter an account name.', createHint: 'Leave the password blank to use this account without a password lock.', resetInvalid: 'Enter a new password.',
    retry: 'Retry', closeConfirmation: 'Cancel confirmation',
  },
  ja: {
    title: 'アカウント管理', unavailable: 'このアカウントではアカウント管理を利用できません。',
    loading: 'アカウント管理を読み込み中…', loadFailed: 'アカウント管理を読み込めません。再操作の前に更新してください。', refresh: 'アカウントを更新', refreshing: '更新中…',
    pending: '会話の作成要求が保留中です。アカウントを変更する前に完了または破棄してください。',
    noAccounts: '追加アカウントはありません。', noList: '現在の権限ではアカウント一覧を表示できません。',
    create: 'アカウントを追加', cancel: 'キャンセル', createName: 'アカウント名', createPassword: '初期パスワード', createSubmit: 'アカウントを作成', creating: '作成中…',
    disable: '無効化', enable: '有効化', reset: 'パスワードをリセット', resetPassword: '新しいパスワード', resetSubmit: 'リセットを実行', resetting: 'リセット中…',
    confirmDisable: 'アカウントを無効化', confirmEnable: 'アカウントを有効化', confirmReset: 'パスワードリセットを確認',
    revoke: '無効化すると現在のログイン資格を失効させます。有効化後は新しくログインできますが、以前のセッションは復元されません。',
    resetRevoke: 'パスワードをリセットすると対象アカウントの現在のログイン資格が失効します。自分のアカウントをリセットした場合も同様です。',
    disabled: '無効', active: '有効', administrator: '管理者', member: 'メンバー',
    created: 'アカウントを作成しました。', updated: 'アカウントを更新しました。',
    mutationUncertain: 'アカウント変更の結果を確認できません。次の変更の前にアカウントを更新して現在の状態を確認してください。',
    refreshAfterChange: '変更は成功しましたが、アカウント情報を更新できません。次の変更の前にアカウントを更新してください。',
    createInvalid: 'アカウント名を入力してください。', createHint: 'パスワードを空欄にすると、パスワードロックなしで利用できます。', resetInvalid: '新しいパスワードを入力してください。',
    retry: '再試行', closeConfirmation: '確認をキャンセル',
  },
} as const

export default function AccountManagement({
  language,
  currentAccountId,
  busy = false,
  onAccountChanged,
  onBusyChange,
  onAccessLost,
}: {
  language: Language
  currentAccountId: string
  busy?: boolean
  onAccountChanged: () => Promise<void>
  onBusyChange?: (busy: boolean) => void
  onAccessLost?: () => void
}) {
  const t = copy[language]
  const [capabilities, setCapabilities] = useState<ManagementCapabilities | null>(null)
  useSettingsCapabilityGuard(currentAccountId, 'management.capabilities', capabilities, onAccessLost)
  const [accounts, setAccounts] = useState<Account[]>([])
  const [loadState, setLoadState] = useState<LoadState>('loading')
  const [loadError, setLoadError] = useState(false)
  const [creationStatus, setCreationStatus] = useState<CreationStatus | null>(null)
  const [creationState, setCreationState] = useState<LoadState>('loading')
  const [mutationBusy, setMutationBusy] = useState(false)
  const [mutationNeedsRefresh, setMutationNeedsRefresh] = useState(false)
  const [mutationError, setMutationError] = useState('')
  const [mutationNotice, setMutationNotice] = useState('')
  const [createOpen, setCreateOpen] = useState(false)
  const [createName, setCreateName] = useState('')
  const [createPassword, setCreatePassword] = useState('')
  const createPasswordInput = useRef<HTMLInputElement | null>(null)
  const [createError, setCreateError] = useState('')
  const [confirmation, setConfirmation] = useState<Confirmation>(null)
  const [resetTarget, setResetTarget] = useState<Account | null>(null)
  const [resetPassword, setResetPassword] = useState('')
  const resetPasswordInput = useRef<HTMLInputElement | null>(null)
  const [resetError, setResetError] = useState('')
  const loadGeneration = useRef(0)
  const mutationGeneration = useRef(0)
  const mutationRunning = useRef(false)
  const mounted = useRef(false)
  const accountIdRef = useRef(currentAccountId)

  const refreshManagement = useCallback(async (releaseMutationGuard = false): Promise<boolean> => {
    const request = ++loadGeneration.current
    setLoadState('loading')
    setLoadError(false)
    setCreationState('loading')
    try {
      const [nextCapabilities, nextCreationStatus] = await Promise.all([
        guiRead('management.capabilities'),
        guiCreationStatus(),
      ])
      if (!mounted.current || request !== loadGeneration.current) return false
      setCapabilities(nextCapabilities)
      setCreationStatus(nextCreationStatus)
      setCreationState('ready')
      if (!nextCapabilities.account_create) {
        setCreateOpen(false)
        setCreatePassword('')
      }
      if (!nextCapabilities.account_get || !nextCapabilities.account_reset) {
        setResetTarget(null)
        setResetPassword('')
      }
      if (!nextCapabilities.account_get || !nextCapabilities.account_disable) setConfirmation(null)
      const nextAccounts = nextCapabilities.account_list
        ? (await guiRead('account.list')).items
        : []
      if (!mounted.current || request !== loadGeneration.current) return false
      setAccounts(nextAccounts)
      setLoadState('ready')
      setLoadError(false)
      if (releaseMutationGuard) setMutationNeedsRefresh(false)
      return true
    } catch {
      if (!mounted.current || request !== loadGeneration.current) return false
      setLoadState('failed')
      setCreationState('failed')
      setLoadError(true)
      setMutationNeedsRefresh(true)
      setCreateOpen(false)
      setCreatePassword('')
      setResetTarget(null)
      setResetPassword('')
      setConfirmation(null)
      return false
    }
  }, [])

  useEffect(() => {
    const accountChanged = accountIdRef.current !== currentAccountId
    accountIdRef.current = currentAccountId
    if (accountChanged) {
      ++loadGeneration.current
      ++mutationGeneration.current
      mutationRunning.current = false
      setCapabilities(null)
      setAccounts([])
      setCreationStatus(null)
      setLoadState('loading')
      setCreationState('loading')
      setMutationBusy(false)
      setMutationNeedsRefresh(false)
      setMutationError('')
      setMutationNotice('')
      setCreatePassword('')
      setResetPassword('')
      setResetTarget(null)
      setConfirmation(null)
      onBusyChange?.(false)
    }
    mounted.current = true
    void refreshManagement()
    return () => {
      mounted.current = false
      ++loadGeneration.current
      ++mutationGeneration.current
      mutationRunning.current = false
      onBusyChange?.(false)
    }
  }, [currentAccountId, onBusyChange, refreshManagement])

  const canMutate = loadState === 'ready'
    && creationState === 'ready'
    && creationStatus?.pending !== true
    && capabilities !== null
    && !busy
    && !mutationBusy
    && !mutationNeedsRefresh

  async function runMutation<T extends AccountManageOperation>(operation: T, args: AccountManagementArgs[T], successLabel: string) {
    if (!canMutate || mutationRunning.current) return
    const request = ++mutationGeneration.current
    mutationRunning.current = true
    setMutationBusy(true)
    setMutationNeedsRefresh(true)
    setMutationError('')
    setMutationNotice('')
    onBusyChange?.(true)
    let committed = false
    try {
      const updated = await guiAccountManage(operation, args)
      committed = true
      if (!mounted.current || request !== mutationGeneration.current) return
      setMutationNotice(successLabel)
      if (updated.id === currentAccountId || operation === 'account.create') await onAccountChanged()
      if (!mounted.current || request !== mutationGeneration.current) return
      const refreshed = await refreshManagement(true)
      if (!refreshed && mounted.current && request === mutationGeneration.current) setMutationError(t.refreshAfterChange)
    } catch {
      if (!mounted.current || request !== mutationGeneration.current) return
      setMutationError(committed ? t.refreshAfterChange : t.mutationUncertain)
    } finally {
      if (mounted.current && request === mutationGeneration.current) {
        mutationRunning.current = false
        setMutationBusy(false)
        onBusyChange?.(false)
      }
    }
  }

  function submitCreate(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (!canMutate || !capabilities?.account_create) return
    const submittedName = createName.trim()
    const submittedPassword = createPassword
    if (createPasswordInput.current) createPasswordInput.current.value = ''
    setCreatePassword('')
    setCreateError('')
    if (!submittedName) {
      setCreateError(t.createInvalid)
      return
    }
    setCreateOpen(false)
    setCreateName('')
    void runMutation('account.create', { name: submittedName, password: submittedPassword }, t.created)
  }

  function submitReset(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (!canMutate || !capabilities?.account_reset || !resetTarget || resetTarget.is_admin) return
    const submittedPassword = resetPassword
    if (resetPasswordInput.current) resetPasswordInput.current.value = ''
    setResetPassword('')
    setResetError('')
    if (!submittedPassword) {
      setResetError(t.resetInvalid)
      return
    }
    const target = resetTarget
    setResetTarget(null)
    void runMutation('account.reset', { id: target.id, password: submittedPassword }, t.updated)
  }

  function confirmDisableChange() {
    if (!confirmation || !canMutate || !capabilities?.account_disable || !capabilities.account_get || confirmation.account.is_admin) return
    const { account, action } = confirmation
    setConfirmation(null)
    void runMutation('account.disable', { id: account.id, disabled: action === 'disable' }, t.updated)
  }

  const managementAvailable = capabilities && (
    capabilities.account_create
    || capabilities.account_list
  )

  return <section className="settings-account-management" data-testid="account-management" aria-labelledby="account-management-title">
    <header className="account-management-heading">
      <h3 id="account-management-title">{t.title}</h3>
      {capabilities && managementAvailable && <button className="post-link" data-testid="account-management-refresh" type="button" onClick={() => void refreshManagement(true)} disabled={loadState === 'loading' || mutationBusy || busy}>{loadState === 'loading' ? t.refreshing : t.refresh}</button>}
    </header>
    {loadState === 'loading' && <p className="account-management-status" data-testid="account-management-loading" role="status">{t.loading}</p>}
    {loadError && <div className="account-management-error"><p role="alert" data-testid="account-management-error">{t.loadFailed}</p><button className="post-link" data-testid="account-management-retry" type="button" onClick={() => void refreshManagement(true)} disabled={mutationBusy || busy}>{t.retry}</button></div>}
    {creationState === 'loading' && loadState === 'ready' && <p className="account-management-status" role="status">{language === 'ja' ? '保留中の作成要求を確認中…' : 'Checking pending creation…'}</p>}
    {creationState === 'failed' && loadState === 'ready' && <div className="account-management-error"><p role="alert">{language === 'ja' ? '保留中の作成要求を確認できません。' : 'Could not check for a pending creation request.'}</p><button className="post-link" type="button" onClick={() => void refreshManagement(true)} disabled={mutationBusy || busy}>{t.retry}</button></div>}
    {creationStatus?.pending && creationState === 'ready' && <p className="account-management-pending" data-testid="account-creation-pending" role="alert">{t.pending}</p>}
    {mutationError && <p className="account-management-error-text" data-testid="account-mutation-error" role="alert">{mutationError}</p>}
    {mutationNotice && <p className="account-management-success" data-testid="account-mutation-notice" role="status">{mutationNotice}</p>}
    {capabilities && loadState === 'ready' && !managementAvailable && <p className="account-management-status">{t.unavailable}</p>}
    {capabilities?.account_create && <div className="account-create-block">
      {!createOpen
        ? <button className="button button-secondary" data-testid="account-create-toggle" type="button" onClick={() => { setCreateOpen(true); setCreateError('') }} disabled={!canMutate}>{t.create}</button>
        : <form className="account-management-form" data-testid="account-create-form" onSubmit={submitCreate}>
            <label className="field"><span>{t.createName}</span><input data-testid="account-create-name" value={createName} onChange={(event) => setCreateName(event.target.value)} autoComplete="off" disabled={!canMutate} /></label>
            <label className="field"><span>{t.createPassword}</span><input ref={createPasswordInput} data-testid="account-create-password" type="password" autoComplete="new-password" value={createPassword} onChange={(event) => setCreatePassword(event.target.value)} disabled={!canMutate} /></label>
            <p className="account-access-hint">{t.createHint}</p>
            {createError && <p className="workspace-error" role="alert">{createError}</p>}
            <div className="account-management-actions"><button className="button button-secondary" type="button" onClick={() => { setCreateOpen(false); setCreateName(''); setCreatePassword(''); setCreateError('') }} disabled={mutationBusy}>{t.cancel}</button><button className="button button-primary" data-testid="account-create-submit" type="submit" disabled={!canMutate}>{mutationBusy ? t.creating : t.createSubmit}</button></div>
          </form>}
    </div>}
    {!capabilities?.account_list && capabilities?.account_create && <p className="account-management-status">{t.noList}</p>}
    {capabilities?.account_list && loadState === 'ready' && <div className="account-list" data-testid="account-list">
      {accounts.length === 0 && <p className="account-management-status">{t.noAccounts}</p>}
      {accounts.map((account) => {
        const canManageAccount = capabilities.account_get && !account.is_admin
        const canDisable = canManageAccount && capabilities.account_disable
        const canReset = canManageAccount && capabilities.account_reset
        const isCurrent = account.id === currentAccountId
        return <article className="account-management-card" data-testid={`account-row-${account.id}`} key={account.id}>
          <div className="account-management-identity"><strong>{account.name}</strong><span>{account.is_admin ? t.administrator : t.member}</span><span className={account.disabled ? 'account-disabled' : 'account-active'}>{account.disabled ? t.disabled : t.active}</span>{isCurrent && <span className="account-current-marker">{language === 'ja' ? '現在のアカウント' : 'Current account'}</span>}</div>
          {(canDisable || canReset) && <div className="account-management-actions">
            {canDisable && <button className="button button-secondary" data-testid={`account-${account.disabled ? 'enable' : 'disable'}-${account.id}`} type="button" onClick={() => { setConfirmation({ account, action: account.disabled ? 'enable' : 'disable' }); setResetTarget(null); setResetPassword('') }} disabled={!canMutate}>{account.disabled ? t.enable : t.disable}</button>}
            {canReset && <button className="button button-secondary" data-testid={`account-reset-${account.id}`} type="button" onClick={() => { setResetTarget(account); setResetPassword(''); setResetError(''); setConfirmation(null) }} disabled={!canMutate}>{t.reset}</button>}
          </div>}
          {confirmation?.account.id === account.id && <div className="account-confirmation" role="group" aria-label={confirmation.action === 'disable' ? t.confirmDisable : t.confirmEnable}>
            <p>{t.revoke}</p>
            <div className="account-management-actions"><button className="button button-secondary" type="button" data-testid="account-confirm-cancel" onClick={() => setConfirmation(null)} disabled={mutationBusy}>{t.cancel}</button><button className="button button-primary" type="button" data-testid="account-confirm-action" onClick={confirmDisableChange} disabled={!canMutate}>{confirmation.action === 'disable' ? t.confirmDisable : t.confirmEnable}</button></div>
          </div>}
          {resetTarget?.id === account.id && <form className="account-management-form account-reset-form" data-testid="account-reset-form" onSubmit={submitReset}>
            <p className="account-management-notice">{t.resetRevoke}</p>
            <label className="field"><span>{t.resetPassword}</span><input ref={resetPasswordInput} data-testid="account-reset-password" type="password" autoComplete="new-password" value={resetPassword} onChange={(event) => setResetPassword(event.target.value)} disabled={!canMutate} /></label>
            {resetError && <p className="workspace-error" role="alert">{resetError}</p>}
            <div className="account-management-actions"><button className="button button-secondary" data-testid="account-reset-cancel" type="button" onClick={() => { setResetTarget(null); setResetPassword(''); setResetError('') }} disabled={mutationBusy}>{t.cancel}</button><button className="button button-primary" data-testid="account-reset-submit" type="submit" disabled={!canMutate}>{mutationBusy ? t.resetting : t.confirmReset}</button></div>
          </form>}
        </article>
      })}
    </div>}
  </section>
}
