// @vitest-environment jsdom
import { act } from 'react'
import { createRoot } from 'react-dom/client'
import { expect, it, vi } from 'vitest'
import { invoke } from '@tauri-apps/api/core'
import BugReportsPanel from './BugReportsPanel'
vi.mock('@tauri-apps/api/core',()=>({invoke:vi.fn()}))
vi.mock('./nativeClosePreferences',()=>({registerUpdateBlocker:()=>()=>{}}))
it('offers four severities, requires redaction and keeps consent controlled by Admin',async()=>{
  vi.mocked(invoke).mockResolvedValue({enabled:false,pending_count:2,pending_ids:[],recent_sent:[]})
  const host=document.createElement('div'),root=createRoot(host)
  await act(async()=>root.render(<BugReportsPanel language="en" accountId="human" isAdmin={false}/>))
  expect(host.textContent).toContain('The Admin controls upload consent')
  expect(host.querySelectorAll('select')[1].querySelectorAll('option')).toHaveLength(4)
  expect(host.querySelector<HTMLButtonElement>('button[type="submit"]')!.disabled).toBe(true)
  expect(host.textContent).toContain('Queued: 2')
  await act(async()=>root.render(<BugReportsPanel language="ja" accountId="admin" isAdmin/>))
  const consent=host.querySelector<HTMLInputElement>('input[type="checkbox"]')!
  await act(async()=>consent.click())
  expect(invoke).toHaveBeenCalledWith('gui_bug_report_consent',{expectedAccountId:'admin',enabled:true})
  expect(invoke).toHaveBeenCalledWith('gui_bug_report_retry')
  await act(async()=>root.unmount())
})
