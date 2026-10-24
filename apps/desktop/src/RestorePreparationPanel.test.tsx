// @vitest-environment jsdom
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import RestorePreparationPanel from './RestorePreparationPanel'
import { guiRestoreStatus, guiPickRestoreBackup, guiPrepareRestore, guiClearRestore, guiApplyRestore, type RestoreStatus } from './bridge'

vi.mock('./bridge', () => ({ guiRestoreStatus: vi.fn(), guiPickRestoreBackup: vi.fn(), guiPrepareRestore: vi.fn(), guiClearRestore: vi.fn(), guiApplyRestore: vi.fn() }))
const chosen: RestoreStatus = { state: 'selected', path: 'C:\\private\\selected.backup', error_code: null, preparation: null, preview: { input_sha256: 'a'.repeat(64), targets: [{project_id:null,target_path:'C:\\private\\RULES.md',revision:'global'}], manifest: {created_at:'2026-10-10T00:00:00Z',schema_version:7,database_bytes:1024,rules:[{project_id:null,source_path:'C:\\old\\RULES.md',revision:'global',content:'<script>not executable</script>'}]} } }
const prepared: RestoreStatus = { ...chosen, state: 'prepared', can_retry: true, preparation: { intent:'fixture',state:'prepared',safety_directory:'C:\\private\\safety',safety_key:'safety-fixture',safety_sha256:'b'.repeat(64) } }
const onBusy = vi.fn()
let host: HTMLDivElement, root: Root
beforeEach(() => {
  vi.resetAllMocks(); vi.mocked(guiRestoreStatus).mockResolvedValue(null)
  host = document.createElement('div'); document.body.append(host); root = createRoot(host)
  ;(globalThis as {IS_REACT_ACT_ENVIRONMENT?: boolean}).IS_REACT_ACT_ENVIRONMENT = true
})
afterEach(async () => { await act(async () => root.unmount()); host.remove() })
function panel(accountId = 'admin') { return <RestorePreparationPanel accountId={accountId} language="en" disabled={false} onBusyChange={onBusy} /> }
function button(name: string) { const found = Array.from(host.querySelectorAll('button')).find(b => b.textContent === name); if (!found) throw new Error(name); return found }
async function click(name: string) { await act(async () => button(name).click()) }
async function review() { await act(async () => host.querySelector<HTMLInputElement>('input[type="checkbox"]')!.click()) }
async function open() { await act(async () => root.render(panel())); await act(async () => host.querySelector('summary')!.click()) }

it('requires review, blocks duplicate save and renders RULES as plain text', async () => {
  vi.mocked(guiPickRestoreBackup).mockResolvedValue(chosen)
  let finish!: (value: RestoreStatus) => void
  vi.mocked(guiPrepareRestore).mockReturnValue(new Promise(resolve => { finish = resolve }))
  await open(); await click('Choose backup file')
  expect(button('Save current state for safety').disabled).toBe(true)
  expect(host.querySelector('pre')!.textContent).toBe('<script>not executable</script>')
  expect(host.querySelector('script')).toBeNull()
  await review(); await click('Save current state for safety'); await click('Save current state for safety')
  expect(guiPrepareRestore).toHaveBeenCalledTimes(1); expect(guiPrepareRestore).toHaveBeenCalledWith('admin')
  expect(onBusy).toHaveBeenCalledWith(true)
  await act(async () => finish(prepared))
  expect(host.textContent).toContain('The current state was saved for safety. The restore has not been applied.')
  vi.mocked(guiClearRestore).mockResolvedValue(null)
  await click('Clear selection (keep safety backups)')
  expect(host.textContent).not.toContain(chosen.path)
})
it('keeps selection on cancellation and reconciles failed save before retry', async () => {
  vi.mocked(guiRestoreStatus).mockResolvedValue(chosen); vi.mocked(guiPickRestoreBackup).mockResolvedValue(null)
  await open(); await click('Choose backup file'); expect(host.textContent).toContain(chosen.path)
  vi.mocked(guiPrepareRestore).mockRejectedValueOnce(new Error('lost response'))
  await review(); await click('Save current state for safety')
  expect(host.querySelector('[role="alert"]')).not.toBeNull()
  expect(host.querySelector<HTMLInputElement>('input')!.disabled).toBe(true)
  vi.mocked(guiRestoreStatus).mockResolvedValue({...chosen,state:'unavailable',can_retry:true,error_code:'RESTORE_INVALID'})
  await click('Check restore preparation')
  expect(host.querySelector<HTMLInputElement>('input')!.disabled).toBe(false)
  await review(); expect(button('Retry the same safety save').disabled).toBe(false)
})
it('ignores old account responses and releases controls for the new account', async () => {
  let finish!: (value: RestoreStatus) => void
  vi.mocked(guiPickRestoreBackup).mockReturnValue(new Promise(resolve => { finish = resolve }))
  await open(); await click('Choose backup file')
  await act(async () => root.render(panel('other-admin')))
  await act(async () => finish(chosen))
  expect(host.textContent).not.toContain(chosen.path)
  expect(button('Choose backup file').disabled).toBe(false)
})

it('requires a separate final confirmation, blocks duplicate apply and requests reauthentication', async () => {
  vi.mocked(guiRestoreStatus).mockResolvedValue(prepared)
  const outcome = vi.fn()
  await act(async () => root.render(<RestorePreparationPanel accountId="admin" language="en" disabled={false} onBusyChange={onBusy} onRestoreFinished={outcome} />))
  await click('Review restore application')
  expect(button('Apply restore and sign in again').disabled).toBe(true)
  expect(host.textContent).toContain('All human and AI login credentials and change cursors will expire')
  await review()
  let finish!: (value: {state:'applied';reauthentication_required:true}) => void
  vi.mocked(guiApplyRestore).mockReturnValue(new Promise(resolve => {finish = resolve}))
  await click('Apply restore and sign in again')
  await click('Apply restore and sign in again')
  expect(guiApplyRestore).toHaveBeenCalledTimes(1)
  await act(async () => finish({state:'applied',reauthentication_required:true}))
  expect(outcome).toHaveBeenCalledWith('applied')
})

it('never retries an uncertain application automatically', async () => {
  vi.mocked(guiRestoreStatus).mockResolvedValue(prepared)
  const outcome = vi.fn()
  await act(async () => root.render(<RestorePreparationPanel accountId="admin" language="en" disabled={false} onBusyChange={onBusy} onRestoreFinished={outcome} />))
  await click('Review restore application'); await review()
  vi.mocked(guiApplyRestore).mockRejectedValueOnce(new Error('lost response'))
  await click('Apply restore and sign in again')
  expect(outcome).toHaveBeenCalledWith('uncertain')
  expect(guiApplyRestore).toHaveBeenCalledTimes(1)
  expect(button('Review restore application').disabled).toBe(true)
})

it('ignores an old account application result', async () => {
  vi.mocked(guiRestoreStatus).mockResolvedValue(prepared)
  const outcome = vi.fn()
  await act(async () => root.render(<RestorePreparationPanel accountId="admin" language="en" disabled={false} onBusyChange={onBusy} onRestoreFinished={outcome} />))
  await click('Review restore application'); await review()
  let finish!: (value: {state:'applied';reauthentication_required:true}) => void
  vi.mocked(guiApplyRestore).mockReturnValue(new Promise(resolve => { finish = resolve }))
  await click('Apply restore and sign in again')
  await act(async () => root.render(panel('other-admin')))
  await act(async () => finish({state:'applied',reauthentication_required:true}))
  expect(outcome).not.toHaveBeenCalled()
})
