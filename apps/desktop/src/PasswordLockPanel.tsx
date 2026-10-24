import { useEffect, useRef, useState, type FormEvent } from 'react'
import { getSnapshot, guiSetPassword } from './bridge'
import type { Account, GuiSnapshot } from './types'
import { announcePasswordChange } from './changeFeedback'

const copy = {
  en: { title: 'Password lock', enabled: 'Password lock is on.', disabled: 'Password lock is off.', unknown: 'Reconnect to check the lock state.', current: 'Current password', password: 'New password', confirm: 'Confirm new password', enable: 'Enable password lock', change: 'Change password', disable: 'Disable password lock', hint: 'Turning off the lock requires your current password. Existing login credentials will be replaced.', invalid: 'Check the current password and enter matching new passwords.', failed: 'Could not change the password lock. Check the current state before retrying.', working: 'Updating…' },
  ja: { title: 'パスワードによるロック', enabled: 'パスワードによるロックは有効です。', disabled: 'パスワードによるロックは無効です。', unknown: '再接続してロック状態を確認してください。', current: '現在のパスワード', password: '新しいパスワード', confirm: '新しいパスワードの確認', enable: 'パスワードロックを有効にする', change: 'パスワードを変更', disable: 'パスワードロックを無効にする', hint: '無効化には現在のパスワードが必要です。現在のログイン資格は切り替わります。', invalid: '現在のパスワードと、新しいパスワードの一致を確認してください。', failed: 'ロック設定を変更できませんでした。現在の状態を確認してから再試行してください。', working: '変更中…' },
}

export default function PasswordLockPanel({ language, account, busy, beginOperation, endOperation, onAccountChanged, onUncertain }: {
  language: 'en' | 'ja'; account: Account; busy: boolean; beginOperation: () => boolean; endOperation: () => void;
  onAccountChanged: (snapshot: GuiSnapshot) => void; onUncertain: () => void;
}) {
  const t = copy[language]
  const [current, setCurrent] = useState('')
  const [password, setPassword] = useState('')
  const [confirmation, setConfirmation] = useState('')
  const [error, setError] = useState('')
  const [working, setWorking] = useState(false)
  const acquired = useRef(false)
  const generation = useRef(0)
  const fields = useRef<HTMLFormElement | null>(null)
  useEffect(() => () => { generation.current++; if (acquired.current) { acquired.current = false; endOperation() } }, [endOperation])

  async function save(disable: boolean) {
    if (busy || acquired.current || typeof account.lock_enabled !== 'boolean') return
    const previous = current
    const next = disable ? '' : password
    if ((account.lock_enabled && !previous.trim()) || (!disable && (!next.trim() || next !== confirmation))) { setError(t.invalid); return }
    if (!beginOperation()) return
    acquired.current = true
    setWorking(true)
    const request = ++generation.current
    fields.current?.querySelectorAll('input').forEach(input => { input.value = '' })
    setCurrent(''); setPassword(''); setConfirmation(''); setError('')
    try {
      const snapshot = await guiSetPassword(account.id, account.lock_enabled ? previous : '', next)
      if (request === generation.current) {
        onAccountChanged(snapshot)
        if (!snapshot.pendingIssue && snapshot.account?.id === account.id) {
          announcePasswordChange(disable ? 'disabled' : account.lock_enabled ? 'changed' : 'enabled')
        }
      }
    } catch {
      if (request !== generation.current) return
      try {
        const snapshot = await getSnapshot()
        if (request !== generation.current) return
        onAccountChanged(snapshot)
        setError(t.failed)
      } catch { if (request === generation.current) onUncertain() }
    } finally {
      if (request === generation.current) { setWorking(false); acquired.current = false; endOperation() }
    }
  }
  function submit(event: FormEvent) { event.preventDefault(); void save(false) }
  const disabled = busy || working
  return <section className="password-lock-settings" aria-label={t.title}>
    <h3>{t.title}</h3>
    <p>{typeof account.lock_enabled !== 'boolean' ? t.unknown : account.lock_enabled ? t.enabled : t.disabled}</p>
    {typeof account.lock_enabled === 'boolean' && <form ref={fields} className="password-lock-form" onSubmit={submit}>
      {account.lock_enabled && <label className="field"><span>{t.current}</span><input type="password" autoComplete="current-password" value={current} onChange={event => setCurrent(event.target.value)} disabled={disabled} /></label>}
      <label className="field"><span>{t.password}</span><input type="password" autoComplete="new-password" value={password} onChange={event => setPassword(event.target.value)} disabled={disabled} /></label>
      <label className="field"><span>{t.confirm}</span><input type="password" autoComplete="new-password" value={confirmation} onChange={event => setConfirmation(event.target.value)} disabled={disabled} /></label>
      <p className="account-access-hint">{t.hint}</p>
      {error && <p className="account-access-error" role="alert">{error}</p>}
      <div className="password-lock-actions"><button className="button button-primary" type="submit" disabled={disabled}>{working ? t.working : account.lock_enabled ? t.change : t.enable}</button>
        {account.lock_enabled && <button className="button button-secondary" type="button" onClick={() => void save(true)} disabled={disabled}>{t.disable}</button>}
      </div>
    </form>}
  </section>
}
