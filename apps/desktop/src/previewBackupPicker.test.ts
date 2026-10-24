// @vitest-environment jsdom
import { afterEach, expect, it, vi } from 'vitest'
import { pickPreviewBackupFile } from './previewBackupPicker'

afterEach(() => { vi.restoreAllMocks(); document.body.innerHTML = '' })
it('opens a filtered picker and returns only the selected name without reading contents', async () => {
  const click = vi.spyOn(HTMLInputElement.prototype, 'click').mockImplementation(() => {})
  const result = pickPreviewBackupFile()
  const input = document.querySelector('input')!
  expect(click).toHaveBeenCalledTimes(1)
  expect(input.accept).toBe('.backup')
  const file = new File(['private bytes'], 'manual.backup')
  const reader = vi.spyOn(FileReader.prototype, 'readAsArrayBuffer')
  Object.defineProperty(input, 'files', { value: [file] })
  input.dispatchEvent(new Event('change'))
  expect(await result).toBe('manual.backup')
  expect(reader).not.toHaveBeenCalled()
  expect(document.querySelector('input')).toBeNull()
})
it('cancels without replacing the previous selection and removes temporary input', async () => {
  vi.spyOn(HTMLInputElement.prototype, 'click').mockImplementation(() => {})
  const result = pickPreviewBackupFile()
  document.querySelector('input')!.dispatchEvent(new Event('cancel'))
  expect(await result).toBeNull()
  expect(document.querySelector('input')).toBeNull()
})
