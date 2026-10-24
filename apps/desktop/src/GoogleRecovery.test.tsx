// @vitest-environment jsdom
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import GoogleRecoverySettings, { GoogleRecoveryForm } from './GoogleRecovery'
import { guiGoogleLink, guiGoogleUnlink, guiGoogleRecover, getSnapshot } from './bridge'
import type { Account, GuiSnapshot } from './types'

vi.mock('./bridge',()=>({guiGoogleLink:vi.fn(),guiGoogleUnlink:vi.fn(),guiGoogleRecover:vi.fn(),getSnapshot:vi.fn()}))
let host:HTMLDivElement, root:Root
const account:Account={id:'account-1',name:'Ada',is_admin:true,disabled:false,role_ids:[],lock_enabled:true,google_linked:false}
beforeEach(()=>{vi.clearAllMocks(); host=document.createElement('div');document.body.append(host);root=createRoot(host);(globalThis as {IS_REACT_ACT_ENVIRONMENT?:boolean}).IS_REACT_ACT_ENVIRONMENT=true})
afterEach(async()=>{await act(async()=>root.unmount());host.remove()})
function input(index:number,value:string) { const field=host.querySelectorAll('input')[index];Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value')?.set?.call(field,value);field.dispatchEvent(new Event('input',{bubbles:true})) }
it('defaults both production surfaces to the localized beta notice without exposing Google actions',async()=>{
  const begin=vi.fn(()=>true), end=vi.fn(), changed=vi.fn(), recovered=vi.fn()
  await act(async()=>root.render(<>
    <GoogleRecoverySettings language="en" account={account} busy={false} beginOperation={begin} endOperation={end} onAccountChanged={changed} onUncertain={vi.fn()}/>
    <GoogleRecoveryForm language="ja" busy={false} onRecovered={recovered}/>
  </>))
  expect(host.querySelectorAll('[role="status"]')).toHaveLength(2)
  expect(host.textContent).toContain('Google recovery is unavailable in the beta. Use your recovery code.')
  expect(host.textContent).toContain('Googleによる復旧はベータ版では利用できません。復旧コードをお使いください。')
  expect(host.querySelector('button, form, input')).toBeNull()
  expect(guiGoogleLink).not.toHaveBeenCalled();expect(guiGoogleUnlink).not.toHaveBeenCalled();expect(guiGoogleRecover).not.toHaveBeenCalled();expect(getSnapshot).not.toHaveBeenCalled()
  expect(begin).not.toHaveBeenCalled();expect(end).not.toHaveBeenCalled();expect(changed).not.toHaveBeenCalled();expect(recovered).not.toHaveBeenCalled()
})
it('requires the current password, clears it before linking, and exposes unlink after confirmation',async()=>{
  const changed=vi.fn(), begin=vi.fn(()=>true), end=vi.fn()
  await act(async()=>root.render(<GoogleRecoverySettings language="en" account={account} busy={false} beginOperation={begin} endOperation={end} onAccountChanged={changed} onUncertain={vi.fn()} testOnlyEnableGoogleRecovery/>))
  await act(async()=>host.querySelector('button')!.click());expect(guiGoogleLink).not.toHaveBeenCalled()
  await act(async()=>input(0,'current-password'))
  let resolve!:(value:GuiSnapshot)=>void
  vi.mocked(guiGoogleLink).mockReturnValue(new Promise(r=>{resolve=r}) as ReturnType<typeof guiGoogleLink>)
  await act(async()=>host.querySelector('button')!.click())
  expect(guiGoogleLink).toHaveBeenCalledWith('account-1','current-password');expect(host.querySelector('input')!.value).toBe('');expect(host.querySelector('button')!.disabled).toBe(true)
  await act(async()=>resolve({setupRequired:false,pendingIssue:false,account:{...account,google_linked:true}}));expect(changed).toHaveBeenCalledOnce();expect(end).toHaveBeenCalledOnce()
  await act(async()=>root.render(<GoogleRecoverySettings language="en" account={{...account,google_linked:true}} busy={false} beginOperation={begin} endOperation={end} onAccountChanged={changed} onUncertain={vi.fn()} testOnlyEnableGoogleRecovery/>))
  await act(async()=>input(0,'current-password'))
  vi.mocked(guiGoogleUnlink).mockResolvedValue({setupRequired:false,pendingIssue:false,account})
  await act(async()=>host.querySelector('button')!.click());expect(guiGoogleUnlink).toHaveBeenCalledWith('account-1','current-password')
})
it('refreshes uncertain linking state without claiming success',async()=>{
  const changed=vi.fn()
  vi.mocked(guiGoogleLink).mockRejectedValue(new Error('timeout'));vi.mocked(getSnapshot).mockResolvedValue({setupRequired:false,pendingIssue:false,account})
  await act(async()=>root.render(<GoogleRecoverySettings language="en" account={{...account,lock_enabled:false}} busy={false} beginOperation={()=>true} endOperation={()=>{}} onAccountChanged={changed} onUncertain={vi.fn()} testOnlyEnableGoogleRecovery/>))
  await act(async()=>host.querySelector('button')!.click());expect(changed).toHaveBeenCalledOnce();expect(host.querySelector('[role="alert"]')!.textContent).toContain('could not be completed')
})
it('validates matching recovery passwords and clears both before native authentication',async()=>{
  const submit=vi.fn(async(operation:()=>Promise<unknown>)=>{await operation()})
  vi.mocked(guiGoogleRecover).mockResolvedValue({setupRequired:false,pendingIssue:false,account})
  await act(async()=>root.render(<GoogleRecoveryForm language="en" busy={false} onRecovered={submit} testOnlyEnableGoogleRecovery/>))
  await act(async()=>host.querySelector('button')!.click())
  await act(async()=>{input(0,'Ada');input(1,'next-password');input(2,'mismatch')})
  await act(async()=>host.querySelector('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true})));expect(guiGoogleRecover).not.toHaveBeenCalled()
  await act(async()=>input(2,'next-password'))
  await act(async()=>host.querySelector('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true})));expect(guiGoogleRecover).toHaveBeenCalledWith('Ada','next-password');expect(host.querySelectorAll('input')[1].value).toBe('');expect(host.querySelectorAll('input')[2].value).toBe('')
})
