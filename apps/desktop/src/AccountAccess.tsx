import { useEffect, useRef, useState, type FormEvent } from 'react'
import { authenticate, getSnapshot, guiCreationStatus, revealRecoveryCode } from './bridge'
import type { Account, CreationStatus, GuiSnapshot } from './types'
import AccountNameInput from './AccountNameInput'

type Language = 'en' | 'ja'

const copy = {
  en: {
    switch: 'Switch account', switchHint: 'Sign in as another local account. Your current account remains active if sign-in fails or you cancel.',
    accountName: 'Account name', password: 'Password', signIn: 'Sign in', cancel: 'Cancel', signingIn: 'Checking and signing in…', working: 'Working…',
    pending: 'A saved conversation creation request is pending. Finish or discard it before switching accounts.',
    statusError: 'Could not verify pending creation. Account switching is blocked; retry after reconnecting.',
    signInError: 'Could not switch accounts. Check the account details and try again.',
    recoveryTitle: 'Admin recovery code', showCode: 'Show recovery code', hideCode: 'Hide recovery code',
    revealError: 'Could not reveal the recovery code. Try again.', recoveryLabel: 'Recovery code',
    stateError: 'Could not verify the current account state. Reconnect before continuing.',
  },
  ja: {
    switch: 'アカウントを切り替え', switchHint: '別のローカルアカウントでサインインします。失敗またはキャンセルしても現在のアカウントを維持します。',
    accountName: 'アカウント名', password: 'パスワード', signIn: 'サインイン', cancel: 'キャンセル', signingIn: '確認してサインイン中…', working: '処理中…',
    pending: '会話の作成要求が保留中です。アカウント切り替えの前に完了または破棄してください。',
    statusError: '保留中の作成要求を確認できません。再接続して確認するまでアカウントを切り替えられません。',
    signInError: 'アカウントを切り替えられませんでした。入力内容を確認してもう一度お試しください。',
    recoveryTitle: '管理者の復旧コード', showCode: '復旧コードを表示', hideCode: '復旧コードを隠す',
    revealError: '復旧コードを表示できません。もう一度お試しください。', recoveryLabel: '復旧コード',
    stateError: '現在のアカウント状態を確認できません。再接続してから続行してください。',
  },
} as const

export default function AccountAccess({
  language,
  account,
  busy = false,
  beginOperation,
  endOperation,
  onAccountChanged,
  onUncertain,
  accountNameHistory = [],
}: {
  language: Language
  account: Account
  busy?: boolean
  beginOperation: () => boolean
  endOperation: () => void
  onAccountChanged: (snapshot: GuiSnapshot) => void
  onUncertain: () => void
  accountNameHistory?: string[]
}) {
  const t = copy[language]
  const [switchOpen, setSwitchOpen] = useState(false)
  const [accountName, setAccountName] = useState('')
  const [password, setPassword] = useState('')
  const [switchError, setSwitchError] = useState('')
  const [recoveryCode, setRecoveryCode] = useState('')
  const [recoveryError, setRecoveryError] = useState('')
  const [operationBusy, setOperationBusy] = useState(false)
  const operationRunning = useRef(false)
  const operationAcquired = useRef(false)
  const mounted = useRef(false)
  const generation = useRef(0)
  const accountId = useRef(account.id)
  const passwordInput = useRef<HTMLInputElement | null>(null)

  useEffect(() => {
    mounted.current = true
    return () => {
      mounted.current = false
      generation.current += 1
      operationRunning.current = false
      if (operationAcquired.current) {
        operationAcquired.current = false
        endOperation()
      }
      if (passwordInput.current) passwordInput.current.value = ''
    }
  }, [endOperation])

  useEffect(() => {
    const accountChanged = accountId.current !== account.id
    if (!accountChanged && account.is_admin) return
    if (accountChanged) accountId.current = account.id
    generation.current += 1
    if (accountChanged) {
      setSwitchOpen(false)
      setAccountName('')
      setPassword('')
      setSwitchError('')
      if (passwordInput.current) passwordInput.current.value = ''
    }
    setRecoveryCode('')
    setRecoveryError('')
    finishOperation()
  }, [account.id, account.is_admin, endOperation])

  function finishOperation() {
    operationRunning.current = false
    setOperationBusy(false)
    if (operationAcquired.current) {
      operationAcquired.current = false
      endOperation()
    }
  }

  function clearSwitchForm() {
    if (passwordInput.current) passwordInput.current.value = ''
    setPassword('')
    setAccountName('')
    setSwitchError('')
  }

  async function submitSwitch(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (busy || operationBusy || operationRunning.current) return
    const submittedName = accountName.trim()
    const submittedPassword = password
    if (passwordInput.current) passwordInput.current.value = ''
    setPassword('')
    setSwitchError('')
    if (!submittedName) {
      setSwitchError(t.signInError)
      return
    }

    if (!beginOperation()) {
      setSwitchError(t.signInError)
      return
    }
    operationAcquired.current = true
    const request = ++generation.current
    operationRunning.current = true
    setOperationBusy(true)
    try {
      let creation: CreationStatus
      try {
        creation = await guiCreationStatus()
      } catch {
        if (mounted.current && request === generation.current) setSwitchError(t.statusError)
        return
      }
      if (!mounted.current || request !== generation.current) return
      if (creation.pending) {
        setSwitchError(t.pending)
        return
      }
      const nextSnapshot = await authenticate('login', submittedName, submittedPassword)
      if (!mounted.current || request !== generation.current) return
      setSwitchOpen(false)
      clearSwitchForm()
      onAccountChanged(nextSnapshot)
    } catch {
      if (!mounted.current || request !== generation.current) return
      try {
        const refreshed = await getSnapshot()
        if (!mounted.current || request !== generation.current) return
        onAccountChanged(refreshed)
        if (refreshed.pendingIssue || refreshed.account?.id !== account.id) {
          setSwitchOpen(false)
          clearSwitchForm()
        } else {
          setSwitchError(t.signInError)
        }
      } catch {
        if (mounted.current && request === generation.current) {
          setSwitchError(t.stateError)
          onUncertain()
        }
      }
    } finally {
      if (mounted.current && request === generation.current) {
        finishOperation()
      }
    }
  }

  async function showRecoveryCode() {
    if (!account.is_admin || busy || operationBusy || operationRunning.current || recoveryCode) return
    const request = ++generation.current
    if (!beginOperation()) return
    operationAcquired.current = true
    operationRunning.current = true
    setOperationBusy(true)
    setRecoveryError('')
    try {
      const result = await revealRecoveryCode()
      if (!mounted.current || request !== generation.current || accountId.current !== account.id) return
      setRecoveryCode(result.recovery_code)
    } catch {
      if (mounted.current && request === generation.current) {
        setRecoveryCode('')
        setRecoveryError(t.revealError)
      }
    } finally {
      if (mounted.current && request === generation.current) {
        finishOperation()
      }
    }
  }

  const controlsBusy = busy || operationBusy

  return <section className="account-access" data-testid="account-access" aria-label={language === 'ja' ? 'アカウントへのアクセス' : 'Account access'}>
    <div className="account-access-disclosure">
      <button className="account-access-toggle" type="button" data-testid="account-switch-toggle" aria-expanded={switchOpen} aria-controls="account-switch-panel" onClick={() => {
        if (controlsBusy) return
        if (switchOpen) clearSwitchForm()
        setSwitchOpen((open) => !open)
      }} disabled={controlsBusy}>{t.switch}</button>
      <p>{t.switchHint}</p>
      {switchOpen && <form id="account-switch-panel" className="account-access-form" data-testid="account-switch-form" onSubmit={submitSwitch}>
        <label className="field"><span>{t.accountName}</span><AccountNameInput history={accountNameHistory} data-testid="account-switch-name" value={accountName} onChange={(event) => setAccountName(event.target.value)} disabled={controlsBusy} /></label>
        <label className="field"><span>{t.password}</span><input ref={passwordInput} data-testid="account-switch-password" type="password" autoComplete="current-password" value={password} onChange={(event) => setPassword(event.target.value)} disabled={controlsBusy} /></label>
        {switchError && <p className="account-access-error" data-testid="account-switch-error" role="alert">{switchError}</p>}
        <div className="account-access-actions"><button className="button button-secondary" type="button" data-testid="account-switch-cancel" onClick={() => { setSwitchOpen(false); clearSwitchForm() }} disabled={controlsBusy}>{t.cancel}</button><button className="button button-primary" type="submit" data-testid="account-switch-submit" disabled={controlsBusy}>{operationBusy ? t.signingIn : t.signIn}</button></div>
      </form>}
    </div>
    {account.is_admin && <div className="account-access-disclosure account-recovery-code">
      <h3>{t.recoveryTitle}</h3>
      {!recoveryCode
        ? <button className="account-access-toggle" type="button" data-testid="recovery-code-toggle" onClick={() => void showRecoveryCode()} disabled={controlsBusy}>{operationBusy ? t.working : t.showCode}</button>
        : <div className="recovery-code-value" data-testid="recovery-code-value"><span>{t.recoveryLabel}</span><code>{recoveryCode}</code><button className="account-access-toggle" type="button" data-testid="recovery-code-hide" onClick={() => { setRecoveryCode(''); setRecoveryError('') }} disabled={controlsBusy}>{t.hideCode}</button></div>}
      {recoveryError && <p className="account-access-error" data-testid="recovery-code-error" role="alert">{recoveryError}</p>}
    </div>}
  </section>
}
