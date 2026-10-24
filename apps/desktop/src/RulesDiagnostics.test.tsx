// @vitest-environment jsdom
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import RulesDiagnostics from './RulesDiagnostics'
import { guiRead, guiRulesCheck } from './bridge'
import type { RulesCapabilities, RulesCheckResult, RulesSubject } from './types'

vi.mock('./bridge', () => ({ guiRead: vi.fn(), guiRulesCheck: vi.fn() }))

const capabilities: RulesCapabilities = {
  global: { read: true, validate: true, save: true, check: true },
  projects: [
    { id: 'project-a', name: 'Project A', read: true, validate: true, save: true, check: true },
    { id: 'project-b', name: 'Project B', read: true, validate: true, save: true, check: false },
    { id: 'project-hidden', name: 'Hidden', read: false, validate: false, save: false, check: true },
  ],
  actions: ['response.create', 'post.get'],
}

const subjects: RulesSubject[] = [
  { id: 'ai-a', project_id: 'project-a', model_name: 'Model A' },
  { id: 'ai-b', project_id: 'project-a', model_name: 'Model B' },
  { id: 'wrong-project', project_id: 'project-b', model_name: 'Other' },
]

let host: HTMLDivElement
let root: Root

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (error: unknown) => void
  const promise = new Promise<T>((done, fail) => { resolve = done; reject = fail })
  return { promise, resolve, reject }
}

async function settle() { await new Promise<void>((resolve) => setTimeout(resolve, 0)) }

function element<T extends HTMLElement = HTMLElement>(testId: string): T {
  const target = host.querySelector<T>(`[data-testid="${testId}"]`)
  if (!target) throw new Error(`Element missing: ${testId}`)
  return target
}

function fill(testId: string, value: string) {
  const field = element<HTMLInputElement | HTMLTextAreaElement>(testId)
  act(() => {
    const prototype = field instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype
    Object.getOwnPropertyDescriptor(prototype, 'value')?.set?.call(field, value)
    field.dispatchEvent(new Event('input', { bubbles: true }))
  })
}

function choose(testId: string, value: string) {
  const field = element<HTMLSelectElement>(testId)
  act(() => {
    Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value')?.set?.call(field, value)
    field.dispatchEvent(new Event('change', { bubbles: true }))
  })
}

async function render(props: { caps?: RulesCapabilities; projectId?: string | null; accountId?: string } = {}) {
  host = document.createElement('div')
  document.body.append(host)
  root = createRoot(host)
  await act(async () => {
    root.render(<RulesDiagnostics
      capabilities={props.caps ?? capabilities}
      currentAccountId={props.accountId ?? 'account-a'}
      language="en"
      rulesProjectId={props.projectId ?? null}
      beginOperation={() => true}
      endOperation={() => undefined}
    />)
    await settle()
  })
  await act(async () => { element('rules-diagnostics-disclosure').querySelector('summary')!.click(); await settle() })
  await settle()
}

beforeEach(() => {
  vi.resetAllMocks()
  document.body.replaceChildren()
  Object.defineProperty(globalThis, 'IS_REACT_ACT_ENVIRONMENT', { value: true, configurable: true })
  vi.mocked(guiRead).mockImplementation(async (operation, args) => {
    if (operation === 'rules.subjects') return { items: subjects.filter((subject) => subject.project_id === args?.project_id) } as never
    throw new Error(`Unexpected read: ${operation}`)
  })
  vi.mocked(guiRulesCheck).mockResolvedValue({ allowed: true, reason: 'allowed' } satisfies RulesCheckResult)
})

afterEach(async () => {
  if (root) await act(async () => root.unmount())
  host?.remove()
})

describe('RulesDiagnostics', () => {
  it('checks saved permissions for a registered AI using operation-specific target and object context', async () => {
    await render()
    expect(vi.mocked(guiRead)).toHaveBeenCalledTimes(1)
    expect(vi.mocked(guiRead)).toHaveBeenCalledWith('rules.subjects', { project_id: 'project-a' })
    expect(element<HTMLSelectElement>('rules-check-project').options).toHaveLength(1)
    expect(element<HTMLSelectElement>('rules-check-actor').options).toHaveLength(3)
    expect(host.textContent).toContain('Unsaved edits are not checked')

    fill('rules-check-target-thread_id', 'thread-1')
    fill('rules-check-target-to', '2')
    fill('rules-check-context', '{"purpose":"review"}')
    await act(async () => { element('rules-check-run').click(); await settle() })

    expect(guiRulesCheck).toHaveBeenCalledWith(
      { project_id: 'project-a', actor_id: 'ai-a', operation: 'response.create', target: { thread_id: 'thread-1', to: 2 } },
      { purpose: 'review' },
    )
    expect(host.querySelector('[data-testid="rules-check-result"]')?.textContent).toContain('Allowed by current effective permissions')
  })

  it('localizes a fixed target or action denial instead of treating it as an ordinary Rules denial', async () => {
    vi.mocked(guiRulesCheck).mockResolvedValueOnce({ allowed: false, reason: 'operation_prohibited' })
    await render()
    choose('rules-check-operation', 'post.get')
    fill('rules-check-target-thread_id', 'thread-9')
    fill('rules-check-target-no', '19')
    await act(async () => { element('rules-check-run').click(); await settle() })

    expect(guiRulesCheck).toHaveBeenCalledWith(
      { project_id: 'project-a', actor_id: 'ai-a', operation: 'post.get', target: { thread_id: 'thread-9', no: 19 } },
      {},
    )
    expect(host.querySelector('[data-testid="rules-check-result"]')?.textContent).toContain('This operation is prohibited')
    expect(host.querySelector('[data-testid="rules-check-result"]')?.textContent).toContain('Denied by current effective permissions')
  })

  it('rejects non-object and over-limit context without calling the checker', async () => {
    await render()
    fill('rules-check-target-thread_id', 'thread-1')
    fill('rules-check-target-to', '1')
    fill('rules-check-context', '[]')
    expect(element('rules-check-context-error').textContent).toContain('Context must be a JSON object')
    expect(element<HTMLButtonElement>('rules-check-run').disabled).toBe(true)
    expect(guiRulesCheck).not.toHaveBeenCalled()

    fill('rules-check-context', JSON.stringify({ note: 'あ'.repeat(22_000) }))
    expect(element('rules-check-context-error').textContent).toContain('64 KiB')
    expect(guiRulesCheck).not.toHaveBeenCalled()
  })

  it('drops a late check result after diagnostic inputs change', async () => {
    const pending = deferred<RulesCheckResult>()
    vi.mocked(guiRulesCheck).mockReturnValueOnce(pending.promise)
    await render()
    fill('rules-check-target-thread_id', 'thread-1')
    fill('rules-check-target-to', '2')
    await act(async () => { element('rules-check-run').click(); await settle() })
    expect(element('rules-check-run').textContent).toBe('Checking…')

    fill('rules-check-context', '{"purpose":"changed"}')
    expect(host.querySelector('[data-testid="rules-check-result"]')).toBeNull()
    await act(async () => { pending.resolve({ allowed: true, reason: 'allowed' }); await settle() })
    expect(host.querySelector('[data-testid="rules-check-result"]')).toBeNull()
  })

  it('does not read actors or run checks after the selected Rules scope loses check permission', async () => {
    const subjectRequest = deferred<{ items: RulesSubject[] }>()
    vi.mocked(guiRead).mockReturnValueOnce(subjectRequest.promise as never)
    await render()
    expect(guiRead).toHaveBeenCalledTimes(1)
    const revoked = { ...capabilities, global: { ...capabilities.global, check: false }, projects: capabilities.projects.map((project) => ({ ...project, check: false })) }
    await act(async () => {
      root.render(<RulesDiagnostics capabilities={revoked} currentAccountId="account-a" language="en" rulesProjectId={null} beginOperation={() => true} endOperation={() => undefined} />)
      await settle()
    })
    expect(element('rules-check-denied').textContent).toContain('Permission checking is not available')
    await act(async () => { subjectRequest.resolve({ items: subjects }); await settle() })
    expect(host.querySelector('[data-testid="rules-check-result"]')).toBeNull()
    expect(guiRulesCheck).not.toHaveBeenCalled()
    expect(guiRead).toHaveBeenCalledTimes(1)
  })

  it('shows no-project and no-actor states without enabling the check', async () => {
    const noProject = { ...capabilities, global: { ...capabilities.global, check: true }, projects: capabilities.projects.map((project) => ({ ...project, check: false })) }
    await render({ caps: noProject })
    expect(element('rules-check-no-projects').textContent).toContain('No readable project')
    expect(guiRead).not.toHaveBeenCalled()
    expect(guiRulesCheck).not.toHaveBeenCalled()
  })

  it('does not enable a check when the authorized project has no registered AI actors', async () => {
    vi.mocked(guiRead).mockResolvedValueOnce({ items: [] } as never)
    await render()
    expect(element('rules-subjects-empty').textContent).toContain('No registered AI actors')
    expect(element<HTMLButtonElement>('rules-check-run').disabled).toBe(true)
    expect(guiRulesCheck).not.toHaveBeenCalled()
  })
})
