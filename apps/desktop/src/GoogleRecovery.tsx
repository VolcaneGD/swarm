import { useEffect, useRef, useState, type FormEvent } from 'react'
import { getSnapshot, guiGoogleLink, guiGoogleUnlink, guiGoogleRecover } from './bridge'
import type { Account, GuiSnapshot } from './types'

const copy = {
  ja: { title:'Googleアカウントによる復旧（任意）', betaUnavailable:'Googleによる復旧はベータ版では利用できません。復旧コードをお使いください。', link:'Googleアカウントを連携', unlink:'Google連携を解除', linked:'Googleアカウントを連携済みです。', unlinked:'Googleアカウントは連携していません。', unknown:'再接続して連携状態を確認してください。', current:'現在のパスワード', hint:'復旧コードも紛失した場合の最終手段です。システムブラウザーでGoogleへログインします。Googleの固有IDと連携先表示用のメールアドレスを端末に保存します。Googleのパスワードやメールの送受信権限は取得しません。', working:'ブラウザーでのGoogle認証を待っています…', wait:'認証待ちは最大4分です。Googleへの通信は、この操作を選んだときだけ行います。', failed:'Google連携を完了できませんでした。現在の状態を確認してから再試行してください。', required:'現在のパスワードを入力してください。', recover:'Googleアカウントで復旧', name:'Swarmアカウント名', password:'新しいパスワード', confirm:'新しいパスワードの確認', recoveryHint:'事前に連携したGoogleアカウントで本人確認し、パスワードを再設定します。既存のログイン資格は失効します。復旧コードをお持ちの場合は、先にコードで復旧できます。', mismatch:'アカウント名と、新しいパスワードの一致を確認してください。' },
  en: { title:'Google account recovery (optional)', betaUnavailable:'Google recovery is unavailable in the beta. Use your recovery code.', link:'Link Google account', unlink:'Unlink Google account', linked:'A Google account is linked.', unlinked:'No Google account is linked.', unknown:'Reconnect to check Google linking.', current:'Current password', hint:'A last resort if you also lose your recovery code. Sign in to Google in your system browser. The Google account ID and an email address for display are saved on this device. Swarm does not receive your Google password or permission to read or send mail.', working:'Waiting for Google authentication in your browser…', wait:'Authentication waits for up to 4 minutes. Google is contacted only when you choose this action.', failed:'Google linking could not be completed. Check the current state before retrying.', required:'Enter your current password.', recover:'Recover with Google account', name:'Swarm account name', password:'New password', confirm:'Confirm new password', recoveryHint:'Verify the Google account linked in advance to reset your password. Existing login credentials will be revoked. Use your recovery code first if you still have it.', mismatch:'Enter an account name and matching new passwords.' },
}
export default function GoogleRecoverySettings({ language, account, busy, beginOperation, endOperation, onAccountChanged, onUncertain, testOnlyEnableGoogleRecovery = false }: {
  language:'en'|'ja'; account:Account; busy:boolean; beginOperation:()=>boolean; endOperation:()=>void; onAccountChanged:(snapshot:GuiSnapshot)=>void; onUncertain:()=>void; testOnlyEnableGoogleRecovery?:boolean;
}) {
  const t = copy[language]
  const [password,setPassword] = useState('')
  const [error,setError] = useState('')
  const [working,setWorking] = useState(false)
  const field = useRef<HTMLInputElement>(null)
  const acquired = useRef(false)
  const generation = useRef(0)
  useEffect(()=>()=> { generation.current++; if(acquired.current) { acquired.current=false; endOperation() } },[endOperation])
  async function update() {
    if(busy || acquired.current || typeof account.google_linked !== 'boolean' || typeof account.lock_enabled !== 'boolean') return
    if(account.lock_enabled && !password.trim()) { setError(t.required); return }
    if(!beginOperation()) return
    acquired.current=true; setWorking(true); setError('')
    const request=++generation.current
    const previous=account.lock_enabled ? password : ''
    if(field.current) field.current.value=''
    setPassword('')
    try {
      const snapshot=await (account.google_linked ? guiGoogleUnlink(account.id,previous) : guiGoogleLink(account.id,previous))
      if(request===generation.current) onAccountChanged(snapshot)
    } catch {
      if(request!==generation.current) return
      try { const snapshot=await getSnapshot(); if(request===generation.current) { onAccountChanged(snapshot); setError(t.failed) } }
      catch { if(request===generation.current) onUncertain() }
    } finally { if(request===generation.current) { acquired.current=false; setWorking(false); endOperation() } }
  }
  function submit(event:FormEvent) { event.preventDefault(); void update() }
  const known=typeof account.google_linked==='boolean' && typeof account.lock_enabled==='boolean'
  if(!testOnlyEnableGoogleRecovery) return <section className="google-recovery-settings" aria-label={t.title}>
    <p className="account-access-hint" role="status">{t.betaUnavailable}</p>
  </section>
  return <section className="google-recovery-settings" aria-label={t.title}>
    <h3>{t.title}</h3>
    <p>{!known ? t.unknown : account.google_linked ? t.linked : t.unlinked}</p>
    {account.google_linked && account.google_email && <p className="google-recovery-email">{account.google_email}</p>}
    <p className="account-access-hint">{t.hint}</p>
    <form className="google-recovery-form" onSubmit={submit}>
      {account.lock_enabled && <label className="field"><span>{t.current}</span><input ref={field} type="password" autoComplete="current-password" value={password} onChange={event=>setPassword(event.target.value)} disabled={busy||working}/></label>}
      {error && <p className="account-access-error" role="alert">{error}</p>}
      <button className="button button-secondary" type="submit" disabled={!known||busy||working}>{working ? t.working : account.google_linked ? t.unlink : t.link}</button>
    </form>
    <p className="account-access-hint">{t.wait}</p>
  </section>
}
export function GoogleRecoveryForm({language,busy,onRecovered,testOnlyEnableGoogleRecovery = false}:{language:'en'|'ja';busy:boolean;onRecovered:(operation:()=>Promise<GuiSnapshot>)=>Promise<void>;testOnlyEnableGoogleRecovery?:boolean}) {
  const t=copy[language]
  const [open,setOpen]=useState(false)
  const [name,setName]=useState('')
  const [password,setPassword]=useState('')
  const [confirm,setConfirm]=useState('')
  const [error,setError]=useState('')
  const form=useRef<HTMLFormElement>(null)
  const pending=useRef(false)
  async function submit(event:FormEvent) {
    event.preventDefault()
    if(busy||pending.current) return
    if(!name.trim()||!password.trim()||password!==confirm) {setError(t.mismatch);return}
    const accountName=name.trim(), next=password
    form.current?.querySelectorAll('input[type="password"]').forEach(input=>{(input as HTMLInputElement).value=''})
    setPassword('');setConfirm('');setError('');pending.current=true
    try { await onRecovered(()=>guiGoogleRecover(accountName,next)) } finally { pending.current=false }
  }
  if(!testOnlyEnableGoogleRecovery) return <section className="google-recovery-settings" aria-label={t.recover}>
    <p className="account-access-hint" role="status">{t.betaUnavailable}</p>
  </section>
  return <section className="google-recovery-settings" aria-label={t.recover}>
    <button type="button" className="text-button" onClick={()=>setOpen(!open)} disabled={busy}>{t.recover}</button>
    {open && <form ref={form} className="google-recovery-form" onSubmit={submit}>
      <p className="account-access-hint">{t.recoveryHint}</p>
      <label className="field"><span>{t.name}</span><input autoComplete="username" value={name} onChange={event=>setName(event.target.value)} disabled={busy}/></label>
      <label className="field"><span>{t.password}</span><input type="password" autoComplete="new-password" value={password} onChange={event=>setPassword(event.target.value)} disabled={busy}/></label>
      <label className="field"><span>{t.confirm}</span><input type="password" autoComplete="new-password" value={confirm} onChange={event=>setConfirm(event.target.value)} disabled={busy}/></label>
      {error && <p role="alert" className="account-access-error">{error}</p>}
      <button className="button button-secondary" type="submit" disabled={busy}>{busy?t.working:language==='ja'?'Googleで本人確認して再設定':'Reset password with Google'}</button>
      <p className="account-access-hint">{t.wait}</p>
    </form>}
  </section>
}
