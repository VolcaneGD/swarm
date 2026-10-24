// @vitest-environment jsdom
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import BackupSettingsPanel from './BackupSettingsPanel'
import { guiBackupStatus, guiSetBackupSettings, guiCreateManualBackup, guiAckManualBackup, type BackupStatus } from './bridge'
vi.mock('./bridge', () => ({ guiStorageInventory:vi.fn(async()=>({archives:[],recovery_bytes:0})),guiDeleteBackupArchive:vi.fn(),guiClearRestoreArtifacts:vi.fn(), guiBackupStatus: vi.fn(), guiSetBackupSettings: vi.fn(), guiCreateManualBackup:vi.fn(),guiAckManualBackup:vi.fn(), guiRestoreStatus:vi.fn(async()=>null),guiPickRestoreBackup:vi.fn(),guiPrepareRestore:vi.fn(),guiClearRestore:vi.fn() }))
let host: HTMLDivElement, root: Root
const status: BackupStatus = { settings: { enabled: true, interval_seconds: 86400, retention: 7, destination: null }, default_destination: 'C:\\private\\backups', journal_error: null, pending_stage: null, schedule: { version: 1, last_success_at: null, pending: null, generations: [], prune: [] } }
beforeEach(() => { vi.resetAllMocks(); host = document.createElement('div'); document.body.append(host); root = createRoot(host); vi.mocked(guiBackupStatus).mockResolvedValue(status) })
afterEach(async () => { await act(async () => root.unmount()); host.remove() })
it('keeps an uncertain manual save for explicit retry and acknowledges its exact intent',async()=>{
  await act(async()=>root.render(<BackupSettingsPanel accountId="admin" language="en" disabled={false}/>))
  vi.mocked(guiCreateManualBackup).mockRejectedValueOnce(new Error('lost response')).mockResolvedValue({intent:'same',state:'completed',key:'manual-same',destination:'C:\\private'})
  const button=()=>Array.from(host.querySelectorAll<HTMLButtonElement>('button')).find(b=>b.textContent==='Save a manual backup' || b.textContent==='Retry the same backup')!
  await act(async()=>button().click())
  expect(host.textContent).toContain('Could not confirm the save')
  await act(async()=>button().click())
  expect(guiCreateManualBackup).toHaveBeenCalledTimes(2)
  vi.mocked(guiAckManualBackup).mockResolvedValue({acknowledged:true})
  await act(async()=>Array.from(host.querySelectorAll<HTMLButtonElement>('button')).find(b=>b.textContent==='Confirm saved result')!.click())
  expect(guiAckManualBackup).toHaveBeenCalledWith('admin','same')
})
it('shows successful generations and keeps disabling available when journal is corrupt', async () => {
  vi.mocked(guiBackupStatus).mockResolvedValue({ ...status, schedule: null, journal_error: 'BACKUP_STATE' })
  await act(async () => root.render(<BackupSettingsPanel accountId="admin" language="ja" disabled={false} />))
  expect(host.textContent).toContain('履歴を確認できません')
  const checkbox = host.querySelector<HTMLInputElement>('input[type="checkbox"]')!
  await act(async () => checkbox.click())
  vi.mocked(guiSetBackupSettings).mockResolvedValue({ ...status.settings, enabled: false })
  const save = Array.from(host.querySelectorAll<HTMLButtonElement>('button')).find(b => b.textContent === 'バックアップ設定を保存')!
  await act(async () => save.click())
  expect(guiSetBackupSettings).toHaveBeenCalledWith('admin', { ...status.settings, enabled: false })
})
it('requires a fresh check after a failed save and discards old account responses', async () => {
  await act(async () => root.render(<BackupSettingsPanel accountId="admin" language="en" disabled={false} />))
  await act(async () => host.querySelector<HTMLInputElement>('input[type="checkbox"]')!.click())
  vi.mocked(guiSetBackupSettings).mockRejectedValue({ code: 'BACKUP_STORAGE' })
  const save = Array.from(host.querySelectorAll<HTMLButtonElement>('button')).find(b => b.textContent === 'Save backup settings')!
  await act(async () => save.click())
  expect(save.disabled).toBe(true)
  expect(host.querySelector('[role="alert"]')?.textContent).toContain('Check current settings')
  let resolve!: (s: BackupStatus) => void
  vi.mocked(guiBackupStatus).mockReturnValueOnce(new Promise(r => { resolve = r })).mockResolvedValue({ ...status, settings: { ...status.settings, retention: 3 } })
  await act(async () => root.render(<BackupSettingsPanel accountId="old" language="en" disabled={false} />))
  await act(async () => root.render(<BackupSettingsPanel accountId="new" language="en" disabled={false} />))
  await act(async () => resolve(status))
  expect(host.querySelector<HTMLInputElement>('input[name="retention"]')?.value).toBe('3')
})
