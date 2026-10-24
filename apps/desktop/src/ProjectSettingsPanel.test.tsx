// @vitest-environment jsdom
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import ProjectSettingsPanel from './ProjectSettingsPanel'
import { guiProjectList, guiProjectRegister } from './bridge'

vi.mock('./bridge', () => ({ guiProjectList: vi.fn(), guiProjectRegister: vi.fn() }))
const project = { id: 'registered-id', name: 'Swarm', canonical_root: 'C:\\Swarm', paths: ['C:\\Swarm'], created_at: 'now' }
let container: HTMLDivElement
let root: Root
const begin = vi.fn(() => true)
const end = vi.fn()
async function settle() { await new Promise<void>((resolve) => setTimeout(resolve, 0)) }
async function render(accountId = 'admin', language: 'ja' | 'en' = 'en') {
  await act(async () => { root.render(<ProjectSettingsPanel accountId={accountId} language={language} disabled={false} beginOperation={begin} endOperation={end} />); await settle() })
}
function input(testId: string, value: string) {
  const field = container.querySelector<HTMLInputElement>(`[data-testid="${testId}"]`)!
  act(() => { Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(field, value); field.dispatchEvent(new Event('input', { bubbles: true })) })
}
async function submit() { await act(async () => { container.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); await settle() }) }
beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(guiProjectList).mockResolvedValue({ items: [] })
  vi.mocked(guiProjectRegister).mockResolvedValue(project)
  container = document.createElement('div'); document.body.append(container); root = createRoot(container)
})
afterEach(() => { act(() => root.unmount()); container.remove() })

it('registers for the selected account, shows an ID, and clears only confirmed inputs', async () => {
  await render('admin', 'ja')
  expect(container.textContent).toContain('登録済みプロジェクトはありません')
  input('project-name', 'Swarm'); input('project-root', 'C:\\Swarm')
  await submit()
  expect(guiProjectRegister).toHaveBeenCalledWith('admin', 'Swarm', 'C:\\Swarm')
  expect(container.querySelector<HTMLInputElement>('input[readonly]')!.value).toBe('registered-id')
  expect(container.querySelector<HTMLInputElement>('[data-testid="project-name"]')!.value).toBe('')
  expect(end).toHaveBeenCalledOnce()
})

it('blocks changes after an uncertain result until a fresh list succeeds, preserving the draft', async () => {
  vi.mocked(guiProjectRegister).mockRejectedValue({ code: 'CONNECTION_ERROR' })
  await render(); input('project-name', 'Swarm'); input('project-root', 'C:\\Swarm'); await submit()
  expect(container.querySelector<HTMLButtonElement>('[data-testid="project-register"]')!.disabled).toBe(true)
  expect(container.querySelector<HTMLInputElement>('[data-testid="project-name"]')!.value).toBe('Swarm')
  await submit(); expect(guiProjectRegister).toHaveBeenCalledOnce()
  vi.mocked(guiProjectList).mockResolvedValue({ items: [project] })
  await act(async () => { container.querySelector<HTMLButtonElement>('[data-testid="project-refresh"]')!.click(); await settle() })
  expect(container.querySelector<HTMLInputElement>('input[readonly]')!.value).toBe('registered-id')
  expect(container.querySelector<HTMLButtonElement>('[data-testid="project-register"]')!.disabled).toBe(false)
})

it('ignores a stale project list after the account changes', async () => {
  let resolve!: (value: { items: typeof project[] }) => void
  vi.mocked(guiProjectList).mockReturnValueOnce(new Promise((done) => { resolve = done }))
  await render('old-admin'); await render('new-admin')
  await act(async () => { resolve({ items: [project] }); await settle() })
  expect(container.querySelector('input[readonly]')).toBeNull()
  expect(guiProjectList).toHaveBeenLastCalledWith('new-admin')
})

it('prevents a duplicate submit while registration is pending', async () => {
  let resolve!: (value: typeof project) => void
  vi.mocked(guiProjectRegister).mockReturnValueOnce(new Promise((done) => { resolve = done }))
  await render(); input('project-name', 'Swarm'); input('project-root', 'C:\\Swarm'); await submit(); await submit()
  expect(guiProjectRegister).toHaveBeenCalledOnce()
  expect(container.querySelector<HTMLButtonElement>('[data-testid="project-register"]')!.disabled).toBe(true)
  await act(async () => { resolve(project); await settle() })
  expect(end).toHaveBeenCalledOnce()
})
