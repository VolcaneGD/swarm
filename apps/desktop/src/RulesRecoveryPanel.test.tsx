// @vitest-environment jsdom
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import RulesRecoveryPanel from './RulesRecoveryPanel'
import { guiRulesManage } from './bridge'
import type { RulesRecoverySnapshot } from './types'

vi.mock('./bridge', () => ({ guiRulesManage: vi.fn() }))
const id = 'a'.repeat(32)
const snapshot: RulesRecoverySnapshot = {
  scope: 'global', project_id: null, recovery_id: id,
  current: { source: null, revision: 'sha256:corrupt', utf8: false, exists: true },
  candidate: { source: '# exact recovery source', revision: 'sha256:candidate', valid: true, message: 'Valid' },
}
let host: HTMLDivElement
let root: Root
const begin = vi.fn(() => true)
const end = vi.fn()
const recovered = vi.fn(async () => {})
const settle = () => new Promise<void>((resolve) => setTimeout(resolve, 0))
function element<T extends HTMLElement>(name: string): T { return host.querySelector<T>(`[data-testid="rules-recovery-${name}"]`)! }
function render(extra = {}) {
  act(() => root.render(<RulesRecoveryPanel language="en" accountId="a" scope="global" draft="my unsaved draft" original="original editor source" canSave blocked={false} beginOperation={begin} endOperation={end} onRecovered={recovered} {...extra} />))
}
function fill(value: string) {
  act(() => {
    const input = element<HTMLInputElement>('id')
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(input, value)
    input.dispatchEvent(new Event('input', { bubbles: true }))
  })
}
async function click(name: string) { await act(async () => { element<HTMLButtonElement>(name).click(); await settle() }) }
async function inspect() { fill(id); await click('inspect') }
beforeEach(() => {
  vi.clearAllMocks()
  host = document.createElement('div'); document.body.append(host); root = createRoot(host)
  vi.mocked(guiRulesManage).mockImplementation(async (operation) => operation === 'rules.recovery.inspect' ? snapshot : { source: snapshot.candidate.source, revision: snapshot.candidate.revision, scope: 'global', project_id: null })
  render()
})
afterEach(() => { act(() => root.unmount()); host.remove() })
describe('explicit Rules recovery comparison', () => {
  it('shows the original, draft, corrupt current revision and exact candidate without automatically applying', async () => {
    await inspect()
    expect(host.textContent).toContain('original editor source')
    expect(host.textContent).toContain('my unsaved draft')
    expect(host.textContent).toContain('not valid UTF-8')
    expect(host.textContent).toContain('sha256:corrupt')
    expect(host.textContent).toContain(snapshot.candidate.source)
    expect(guiRulesManage).toHaveBeenCalledExactlyOnceWith('rules.recovery.inspect', { recovery_id: id })
    expect(element('apply')).toBeNull()
  })
  it('requires explicit review then binds the displayed current and candidate revisions, keeping the draft', async () => {
    await inspect(); await click('review'); await click('apply')
    expect(guiRulesManage).toHaveBeenLastCalledWith('rules.recovery.apply', { recovery_id: id, recovery_revision: 'sha256:candidate', source: snapshot.candidate.source, if_revision: 'sha256:corrupt' })
    expect(recovered).toHaveBeenCalledOnce()
    expect(host.textContent).toContain('draft is retained')
    expect(end).toHaveBeenCalledTimes(2)
  })
  it('refuses to report success when apply returns different content', async () => {
    await inspect(); await click('review')
    vi.mocked(guiRulesManage).mockResolvedValueOnce({ source: 'other', revision: snapshot.candidate.revision, scope: 'global', project_id: null })
    await click('apply')
    expect(recovered).not.toHaveBeenCalled()
    expect(host.querySelector('[role="alert"]')?.textContent).toContain('Recovery failed')
    expect(element('comparison')).toBeNull()
  })
  it('invalidates comparison when the Recovery ID changes and denies traversal IDs', async () => {
    await inspect(); fill('../RULES.md')
    expect(element('comparison')).toBeNull()
    expect(element<HTMLButtonElement>('inspect').disabled).toBe(true)
  })
  it('allows comparison for readers but blocks apply without save permission or for an invalid candidate', async () => {
    render({ canSave: false }); await inspect()
    expect(element<HTMLButtonElement>('review').disabled).toBe(true)
    render({ canSave: true })
    vi.mocked(guiRulesManage).mockResolvedValueOnce({ ...snapshot, candidate: { ...snapshot.candidate, valid: false } })
    await click('inspect')
    expect(element<HTMLButtonElement>('review').disabled).toBe(true)
  })
  it('drops late inspection responses after an account or scope change', async () => {
    let resolve!: (value: RulesRecoverySnapshot) => void
    vi.mocked(guiRulesManage).mockReturnValueOnce(new Promise<RulesRecoverySnapshot>((done) => { resolve = done }))
    fill(id)
    act(() => element<HTMLButtonElement>('inspect').click())
    render({ accountId: 'b', scope: 'project-b' })
    await act(async () => { resolve(snapshot); await settle() })
    expect(element('comparison')).toBeNull()
    expect(element<HTMLInputElement>('id').value).toBe('')
    expect(end).toHaveBeenCalledOnce()
  })
  it('clears a rejected comparison and requires a new comparison after a stale current-file refusal', async () => {
    await inspect(); await click('review')
    vi.mocked(guiRulesManage).mockRejectedValueOnce({ code: 'RULES_CHANGED' })
    await click('apply')
    expect(host.querySelector('[role="alert"]')?.textContent).toContain('draft is retained')
    expect(element('apply')).toBeNull()
    expect(recovered).not.toHaveBeenCalled()
  })
})
