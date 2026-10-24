import { useEffect, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { registerUpdateBlocker } from './nativeClosePreferences'

type Status={enabled:boolean;pending_count:number;pending_ids:string[];recent_sent:string[]}
const categories={permissions:['権限','Permissions'],accounts:['アカウント','Accounts'],gui:['GUI','GUI'],cli:['CLI','CLI'],runtime:['常駐処理','Runtime'],data:['データ','Data'],backup_restore:['バックアップ・復元','Backup & restore'],notifications:['通知','Notifications'],updates:['更新','Updates'],installation:['導入','Installation'],ai_integration:['AI連携','AI integration'],security:['セキュリティ','Security'],other:['その他','Other']}
const severities={critical:['重大：情報漏えい・データ消失など','Critical: disclosure or data loss'],high:['高：主要な操作ができない','High: major operation blocked'],medium:['中：一部に不具合・回避策あり','Medium: partial issue or workaround'],low:['低：見た目・文言など','Low: appearance or wording']}
const empty={summary:'',steps:'',expected:'',actual:'',severity_reason:'',error_code:''}
export default function BugReportsPanel({language,accountId,isAdmin}:{language:'ja'|'en';accountId:string;isAdmin:boolean}){
  const ja=language==='ja', n=ja?0:1
  const [status,setStatus]=useState<Status|null>(null),[busy,setBusy]=useState(false),[error,setError]=useState(''),[result,setResult]=useState('')
  const [form,setForm]=useState(empty),[category,setCategory]=useState('gui'),[severity,setSeverity]=useState('medium'),[redacted,setRedacted]=useState(false)
  const active=useRef(true),inFlight=useRef(false),dirty=Object.values(form).some(value=>value.length>0)
  const attempt=useRef<{signature:string;id:string}|null>(null)
  useEffect(()=>registerUpdateBlocker(()=>dirty||busy),[dirty,busy])
  useEffect(()=>{active.current=true;void invoke<Status>('gui_bug_report_status').then(value=>{if(active.current)setStatus(value)}).catch(()=>{if(active.current)setError(ja?'報告設定を読み込めませんでした。':'Could not load report settings.')});return()=>{active.current=false}},[accountId,ja])
  async function action(task:()=>Promise<void>){if(inFlight.current)return;inFlight.current=true;setBusy(true);setError('');try{await task()}catch{if(active.current)setError(ja?'操作できませんでした。報告の必須項目・匿名化・接続状況を確認してください。':'Could not process the report. Check required fields, redaction and connection.')}finally{inFlight.current=false;if(active.current)setBusy(false)}}
  async function consent(enabled:boolean){await action(async()=>{const value=await invoke<Status>('gui_bug_report_consent',{expectedAccountId:accountId,enabled});if(active.current)setStatus(value);if(enabled)await invoke('gui_bug_report_retry')})}
  async function submit(event:React.FormEvent){event.preventDefault();await action(async()=>{
    const signature=JSON.stringify({category,severity,form,redacted})
    if(attempt.current?.signature!==signature)attempt.current={signature,id:crypto.randomUUID()}
    const report={schema_version:1,report_id:attempt.current.id,app_version:'0.1.1',category,severity,...form,steps:form.steps.split('\n').map(s=>s.trim()).filter(Boolean),reporter:'human',redacted,consent_version:1}
    const value=await invoke<{report_id:string;delivery:string}>('gui_bug_report_submit',{report})
    attempt.current=null
    if(active.current){setResult(`${value.report_id} — ${value.delivery==='sent'?(ja?'送信済み':'Sent'):(ja?'ローカル保存・送信待ち':'Saved locally; queued')}`);setForm(empty);setRedacted(false);setStatus(await invoke<Status>('gui_bug_report_status'))}
  })}
  return <section className="settings-preferences bug-report-panel" aria-label={ja?'不具合報告':'Bug reports'}>
    <h3>{ja?'不具合報告':'Bug reports'}</h3>
    <p>{ja?'不具合は種類・深刻度・再現手順を揃えて記録します。同意前や通信できない間は端末内へ保存し、同意後はCloudflareへ自動送信します。送信先の報告は開発者だけが閲覧できます。':'Reports include category, severity and reproduction steps. They stay on this device before consent or while offline, and upload automatically to Cloudflare after consent. Only developers can read uploaded reports.'}</p>
    <p>{ja?'会話本文・パスワード・復旧コード・資格情報・個人名・実際のファイルパスを記入しないでください。架空の例や要約を使ってください。':'Do not include conversation bodies, passwords, recovery codes, credentials, names or real file paths. Use synthetic examples and summaries.'}</p>
    {status&&<><p role="status">{ja?'自動送信':'Automatic upload'}: {status.enabled?(ja?'有効':'On'):(ja?'無効（端末内保存のみ）':'Off (local only)')} · {ja?'送信待ち':'Queued'}: {status.pending_count}</p>
    {isAdmin?<label className="checkbox-label"><input type="checkbox" checked={status.enabled} disabled={busy} onChange={event=>void consent(event.target.checked)}/>{ja?'上記の送信先・内容を理解し、匿名化した不具合報告の自動送信に同意する':'I understand the destination and contents and consent to automatic upload of redacted bug reports'}</label>:<p>{ja?'自動送信の同意はAdminが設定します。':'The Admin controls upload consent.'}</p>}
    <button type="button" className="button" disabled={busy||!status.enabled} onClick={()=>void action(async()=>{await invoke('gui_bug_report_retry');setStatus(await invoke<Status>('gui_bug_report_status'))})}>{ja?'送信待ちを再送':'Retry queued reports'}</button></>}
    <form className="bug-report-form" onSubmit={event=>void submit(event)}>
      <div className="bug-report-classification"><label>{ja?'種類':'Category'}<select value={category} disabled={busy} onChange={e=>setCategory(e.target.value)}>{Object.entries(categories).map(([key,text])=><option key={key} value={key}>{text[n]}</option>)}</select></label><label>{ja?'深刻度':'Severity'}<select value={severity} disabled={busy} onChange={e=>setSeverity(e.target.value)}>{Object.entries(severities).map(([key,text])=><option key={key} value={key}>{text[n]}</option>)}</select></label></div>
      {([['summary',ja?'概要':'Summary',160],['steps',ja?'再現手順（1行に1手順）':'Steps (one per line)',4000],['expected',ja?'期待する結果':'Expected result',2000],['actual',ja?'実際の結果':'Actual result',2000],['severity_reason',ja?'深刻度の判断理由':'Severity rationale',500],['error_code',ja?'エラーコード（任意）':'Error code (optional)',64]] as const).map(([key,label,max])=><label key={key}>{label}<textarea rows={key==='summary'||key==='error_code'?1:3} required={key!=='error_code'} maxLength={max} value={form[key]} disabled={busy} onChange={e=>setForm({...form,[key]:e.target.value})}/></label>)}
      <label className="checkbox-label"><input type="checkbox" checked={redacted} disabled={busy} onChange={e=>setRedacted(e.target.checked)} required/>{ja?'私有の本文・個人情報・秘密を含めていないことを確認しました':'I checked that the report contains no private content, personal details or secrets'}</label>
      <button className="button button-primary" disabled={busy||!redacted} type="submit">{busy?(ja?'処理中…':'Processing…'):(ja?'不具合を報告':'Report bug')}</button>
    </form>
    {result&&<p role="status">{result}</p>}{error&&<p role="alert">{error}</p>}
  </section>
}
