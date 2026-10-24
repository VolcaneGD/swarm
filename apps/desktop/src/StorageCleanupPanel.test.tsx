// @vitest-environment jsdom
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { beforeEach, afterEach, it, expect, vi } from 'vitest'
import StorageCleanupPanel from './StorageCleanupPanel'
import { guiStorageInventory, guiDeleteBackupArchive, guiClearRestoreArtifacts } from './bridge'
vi.mock('./bridge', () => ({ guiStorageInventory: vi.fn(), guiDeleteBackupArchive: vi.fn(), guiClearRestoreArtifacts: vi.fn() }))
let host: HTMLDivElement, root: Root
const archive = { directory: 'C:\\Private\\backups', key: 'manual-fixture', bytes: 2048, deletable: true }
beforeEach(() => { vi.resetAllMocks(); host = document.createElement('div'); document.body.append(host); root = createRoot(host); vi.mocked(guiStorageInventory).mockResolvedValue({ archives: [archive], recovery_bytes: 1024 }) })
afterEach(async () => { await act(async () => root.unmount()); host.remove() })
const button = (label: string) => Array.from(host.querySelectorAll('button')).find(b => b.textContent === label)!
const render = (id = 'admin') => act(async () => root.render(<StorageCleanupPanel accountId={id} language="en" disabled={false} />))
it('requires review, supports cancellation and deletes only the reviewed archive', async () => {
  await render()
  expect(host.textContent).toContain('2 KiB')
  expect(host.textContent).toContain('Restore preparation and safety copies: 1 KiB')
  await act(async () => button('Permanently delete').click())
  expect(guiDeleteBackupArchive).not.toHaveBeenCalled()
  expect(host.querySelector('[role="alertdialog"]')?.textContent).toContain(archive.directory)
  await act(async () => button('Cancel').click())
  expect(guiDeleteBackupArchive).not.toHaveBeenCalled()
  await act(async () => button('Permanently delete').click())
  vi.mocked(guiDeleteBackupArchive).mockResolvedValue({ deleted: true })
  await act(async () => button('Delete permanently').click())
  expect(guiDeleteBackupArchive).toHaveBeenCalledWith('admin', archive)
})
it('preserves the review and reports a failed deletion', async () => {
  await render()
  vi.mocked(guiDeleteBackupArchive).mockRejectedValue(new Error('CONFLICT'))
  await act(async () => button('Permanently delete').click())
  await act(async () => button('Delete permanently').click())
  expect(host.querySelector('[role="alert"]')).not.toBeNull()
  expect(host.querySelector('[role="alertdialog"]')).not.toBeNull()
})
it('protects pending archives and requires separate explicit recovery cleanup review', async () => {
  vi.mocked(guiStorageInventory).mockResolvedValue({ archives: [{ ...archive, deletable: false }], recovery_bytes: 1024 })
  await render()
  expect(button('Permanently delete').disabled).toBe(true)
  await act(async () => button('Delete restore artifacts').click())
  expect(guiClearRestoreArtifacts).not.toHaveBeenCalled()
  expect(host.querySelector('[role="alertdialog"]')?.textContent).toContain('all restore preparations')
  vi.mocked(guiClearRestoreArtifacts).mockResolvedValue({ deleted: true })
  await act(async () => button('Delete permanently').click())
  expect(guiClearRestoreArtifacts).toHaveBeenCalledWith('admin')
})
it('discards a review when the selected account changes', async () => {
  await render()
  await act(async () => button('Permanently delete').click())
  await render('another-admin')
  expect(host.querySelector('[role="alertdialog"]')).toBeNull()
  expect(guiDeleteBackupArchive).not.toHaveBeenCalled()
})
