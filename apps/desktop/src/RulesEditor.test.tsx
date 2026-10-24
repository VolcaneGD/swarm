// @vitest-environment jsdom
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import RulesEditor from './RulesEditor'
import { guiCreationStatus, guiRead, guiRulesManage } from './bridge'
import type { CreationStatus, RulesCapabilities, RulesDocument, RulesValidation } from './types'

vi.mock('./bridge', () => ({ guiRead: vi.fn(), guiCreationStatus: vi.fn(), guiRulesManage: vi.fn() }))

const advancedSource = [
  '# Human-authored intent and notes.',
  'Keep this prose and the advanced condition byte-for-byte.',
  '',
  '```swarm-rules',
  'schema_version: 1',
  'defaults:',
  '  ai_read: true',
  'rules:',
  '  - id: "existing-review-rule"',
  '    effect: allow',
  '    subjects:',
  '      projects: ["project-a"]',
  '    actions: ["response.create"]',
  '    scope:',
  '      threads: ["thread-old"]',
  '    when:',
  '      all:',
  '        - field: "context.purpose"',
  '          op: eq',
  '          value: "review"',
  '# Preserve this comment before the next top-level key.',
  'access:',
  '  boards: []',
  '  threads: []',
  '```',
  '',
  'End of human notes.',
].join('\n')

const projectSource = '# Project-specific notes.\n\n```swarm-rules\nschema_version: 1\nrules: []\n```\n'
const projectCapabilities: RulesCapabilities = {
  global: { read: true, validate: true, save: true },
  projects: [{ id: 'project-a', name: 'Project A', read: true, validate: true, save: true }],
  actions: ['response.create', 'post.edit'],
}

let host: HTMLDivElement
let root: Root
let accountId = 'account-a'
let documentByScope: Record<string, RulesDocument> = {}
let currentCapabilities: RulesCapabilities = projectCapabilities

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (error: unknown) => void
  const promise = new Promise<T>((done, fail) => { resolve = done; reject = fail })
  return { promise, resolve, reject }
}

async function settle() { await new Promise<void>((resolve) => setTimeout(resolve, 0)) }

function testElement<T extends HTMLElement = HTMLElement>(testId: string): T {
  const element = host.querySelector<T>(`[data-testid="${testId}"]`)
  if (!element) throw new Error(`Element missing: ${testId}`)
  return element
}

function fill(testId: string, value: string) {
  const field = testElement<HTMLInputElement | HTMLTextAreaElement>(testId)
  act(() => {
    const prototype = field instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype
    Object.getOwnPropertyDescriptor(prototype, 'value')?.set?.call(field, value)
    field.dispatchEvent(new Event('input', { bubbles: true }))
  })
}

function choose(testId: string, value: string) {
  const field = testElement<HTMLSelectElement>(testId)
  act(() => {
    const setter = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value')?.set
    setter?.call(field, value)
    field.dispatchEvent(new Event('change', { bubbles: true }))
  })
}

async function render(nextAccount = accountId) {
  accountId = nextAccount
  host = document.createElement('div')
  document.body.append(host)
  root = createRoot(host)
  await act(async () => {
    root.render(<RulesEditor language="en" currentAccountId={accountId} beginOperation={() => true} endOperation={() => undefined} onRulesChanged={async () => undefined} />)
    await settle()
  })
  await act(async () => { testElement('rules-editor-disclosure').click(); await settle() })
  await settle()
}

async function rerender(nextAccount: string, language: 'en' | 'ja' = 'en') {
  accountId = nextAccount
  await act(async () => {
    root.render(<RulesEditor key={accountId} language={language} currentAccountId={accountId} beginOperation={() => true} endOperation={() => undefined} onRulesChanged={async () => undefined} />)
    await settle()
  })
  await act(async () => { testElement('rules-editor-disclosure').click(); await settle() })
  await settle()
}

async function enterFormRule() {
  await act(async () => { testElement('rules-tab-form').click(); await settle() })
  fill('rules-id', 'allow-thread-response')
  choose('rules-operation', 'response.create')
  fill('rules-subject-project-id', 'project-a')
  choose('rules-target-scope', 'threads')
  fill('rules-target-id', 'thread-a')
  fill('rules-condition-field', 'context.purpose')
  choose('rules-condition-operator', 'eq')
  fill('rules-condition-json', '"review"')
  await act(async () => { testElement<HTMLButtonElement>('rules-add-rule').click(); await settle() })
  await act(async () => { testElement('rules-tab-text').click(); await settle() })
}

beforeEach(() => {
  vi.resetAllMocks()
  document.body.replaceChildren()
  Object.defineProperty(globalThis, 'IS_REACT_ACT_ENVIRONMENT', { value: true, configurable: true })
  documentByScope = {
    global: { source: advancedSource, revision: 'global-r1', scope: 'global', project_id: null },
    'project-a': { source: projectSource, revision: 'project-r1', scope: 'project', project_id: 'project-a' },
  }
  currentCapabilities = projectCapabilities
  vi.mocked(guiRead).mockImplementation(async (operation, args) => {
    if (operation === 'rules.capabilities') return currentCapabilities as never
    if (operation === 'rules.get') return documentByScope[String(args?.project_id || 'global')] as never
    throw new Error(`Unexpected read: ${operation}`)
  })
  vi.mocked(guiCreationStatus).mockResolvedValue({ pending: false, operation: null, expired: false } satisfies CreationStatus)
  vi.mocked(guiRulesManage).mockImplementation(async (operation) => operation === 'rules.validate'
    ? { valid: true, message: 'Valid draft', actions: ['response.create'] } as never
    : { source: advancedSource, revision: 'global-r1', scope: 'global', project_id: null } as never)
})

afterEach(async () => {
  if (root) await act(async () => root.unmount())
  host?.remove()
})

describe('RulesEditor', () => {
  it('appends a form rule while preserving comments and existing advanced source verbatim', async () => {
    await render()
    await enterFormRule()
    const source = testElement<HTMLTextAreaElement>('rules-source').value

    const oldRuleStart = advancedSource.indexOf('  - id: "existing-review-rule"')
    const nextTopLevelKey = advancedSource.indexOf('# Preserve this comment before the next top-level key.')
    expect(source).toContain(advancedSource.slice(oldRuleStart, nextTopLevelKey))
    expect(source).toContain('# Preserve this comment before the next top-level key.')
    expect(source).toContain('        - field: "context.purpose"\n          op: eq\n          value: "review"')
    expect(source).toContain('  - id: "allow-thread-response"\n    effect: allow\n    subjects:\n      projects: ["project-a"]\n    actions: ["response.create"]\n    scope:\n      threads: ["thread-a"]')
    expect(source).toContain('          value: "review"')
    expect(source.endsWith('End of human notes.')).toBe(true)
    expect(guiRulesManage).not.toHaveBeenCalled()
  })

  it('requires explicit confirmation before replacing a non-empty document with the example template', async () => {
    await render()
    const before = testElement<HTMLTextAreaElement>('rules-source').value
    await act(async () => { testElement('rules-insert-example').click(); await settle() })

    expect(host.querySelector('[aria-labelledby="rules-template-title"]')).not.toBeNull()
    expect(testElement<HTMLTextAreaElement>('rules-source').value).toBe(before)
    expect(guiRulesManage).not.toHaveBeenCalled()
  })

  it('does not fetch source for scopes without read permission', async () => {
    currentCapabilities = {
      global: { read: false, validate: false, save: false },
      projects: [{ id: 'project-a', name: 'Project A', read: false, validate: false, save: false }],
      actions: ['response.create'],
    }
    await render()
    expect(host.querySelector('[data-testid="rules-scope-denied"]')).not.toBeNull()
    expect(vi.mocked(guiRead).mock.calls.filter(([operation]) => operation === 'rules.get')).toHaveLength(0)
  })

  it('discards a deferred validation result after the selected scope changes', async () => {
    const validation = deferred<RulesValidation>()
    vi.mocked(guiRulesManage).mockReturnValueOnce(validation.promise as never)
    await render()
    fill('rules-source', `${advancedSource}\n# unsaved draft`)
    await act(async () => { testElement('rules-validate').click(); await settle() })
    expect(guiRulesManage).toHaveBeenCalledWith('rules.validate', expect.objectContaining({ source: `${advancedSource}\n# unsaved draft` }))

    await act(async () => { choose('rules-scope-select', 'project-a'); await settle() })
    expect(testElement<HTMLTextAreaElement>('rules-source').value).toBe(projectSource)
    await act(async () => { validation.resolve({ valid: true, message: 'Stale result', actions: ['response.create'] }); await settle() })
    expect(host.querySelector('[data-testid="rules-validation-summary"]')).toBeNull()
    expect(testElement<HTMLButtonElement>('rules-save').disabled).toBe(true)
  })

  it('rechecks changed permissions before validation and retains the local draft when access is revoked', async () => {
    await render()
    const draft = `${advancedSource}\n# Local draft must remain available.`
    fill('rules-source', draft)
    currentCapabilities = {
      ...projectCapabilities,
      global: { ...projectCapabilities.global, validate: false },
    }
    await act(async () => { testElement('rules-validate').click(); await settle() })

    expect(vi.mocked(guiRulesManage).mock.calls.filter(([operation]) => operation === 'rules.validate')).toHaveLength(0)
    expect(testElement<HTMLTextAreaElement>('rules-source').value).toBe(draft)
    expect(host.querySelector('[role="alert"]')?.textContent).toContain('no longer grants')
  })

  it('rechecks save permission immediately before the confirmed write', async () => {
    await render()
    const draft = `${advancedSource}\n# Save permission is checked again.`
    fill('rules-source', draft)
    await act(async () => { testElement('rules-validate').click(); await settle() })
    await act(async () => { testElement('rules-save').click(); await settle() })
    currentCapabilities = {
      ...projectCapabilities,
      global: { ...projectCapabilities.global, save: false },
    }
    await act(async () => { [...host.querySelectorAll('button')].find((button) => button.textContent?.trim() === 'Save exact validated draft')!.click(); await settle() })
    expect(vi.mocked(guiRulesManage).mock.calls.filter(([operation]) => operation === 'rules.save')).toHaveLength(0)
    expect(testElement<HTMLTextAreaElement>('rules-source').value).toBe(draft)
    expect(host.querySelector('[role="alert"]')?.textContent).toContain('no longer grants')
  })

  it('blocks validation and save while a creation request is pending', async () => {
    vi.mocked(guiCreationStatus).mockResolvedValue({ pending: true, operation: 'post.create', expired: false })
    await render()
    fill('rules-source', `${advancedSource}\n# Must wait for creation resolution.`)
    expect(testElement<HTMLButtonElement>('rules-validate').disabled).toBe(true)
    expect(testElement<HTMLButtonElement>('rules-save').disabled).toBe(true)
    expect(host.textContent).toContain('A conversation creation request is pending')
    expect(guiRulesManage).not.toHaveBeenCalled()
  })

  it('invalidates a successful validation when the source is edited', async () => {
    await render()
    fill('rules-source', `${advancedSource}\n# Draft one.`)
    expect(host.querySelector('[data-testid="rules-save-reason"]')?.textContent).toContain('Validate draft')
    await act(async () => { testElement('rules-validate').click(); await settle() })
    expect(host.querySelector('[data-testid="rules-validation-summary"]')).not.toBeNull()
    expect(host.querySelector('[data-testid="rules-save-reason"]')).toBeNull()
    expect(testElement<HTMLButtonElement>('rules-save').disabled).toBe(false)
    fill('rules-source', `${advancedSource}\n# Draft two.`)
    expect(host.querySelector('[data-testid="rules-validation-summary"]')).toBeNull()
    expect(testElement<HTMLButtonElement>('rules-save').disabled).toBe(true)
    expect(host.querySelector('[data-testid="rules-save-reason"]')?.textContent).toContain('Revalidate')
  })

  it('restores an unsaved scope draft when switching away and back', async () => {
    await render()
    const draft = `${advancedSource}\n# Draft retained per scope.`
    fill('rules-source', draft)
    await act(async () => { choose('rules-scope-select', 'project-a'); await settle() })
    expect(testElement<HTMLTextAreaElement>('rules-source').value).toBe(projectSource)
    await act(async () => { choose('rules-scope-select', 'global'); await settle() })
    expect(testElement<HTMLTextAreaElement>('rules-source').value).toBe(draft)
  })

  it('ignores a deferred saved document from the prior account', async () => {
    const firstDocument = deferred<RulesDocument>()
    let reads = 0
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'rules.capabilities') return projectCapabilities as never
      if (operation === 'rules.get') {
        reads += 1
        if (reads === 1) return firstDocument.promise as never
        return { source: projectSource, revision: 'account-b-r1', scope: 'global', project_id: null } as never
      }
      throw new Error(`Unexpected read: ${operation}`)
    })
    await render('account-a')
    await rerender('account-b')
    await act(async () => { firstDocument.resolve({ source: advancedSource, revision: 'account-a-r1', scope: 'global', project_id: null }); await settle() })
    expect(testElement<HTMLTextAreaElement>('rules-source').value).toBe(projectSource)
    expect(testElement<HTMLTextAreaElement>('rules-source').value).not.toContain('account-a-r1')
  })

  it('validates and reviews the exact draft, and never auto-resubmits an uncertain save', async () => {
    const onRulesChanged = vi.fn(async () => undefined)
    await act(async () => {
      host = document.createElement('div')
      document.body.append(host)
      root = createRoot(host)
      root.render(<RulesEditor language="en" currentAccountId="account-a" beginOperation={() => true} endOperation={() => undefined} onRulesChanged={onRulesChanged} />)
      await settle()
    })
    await act(async () => { testElement('rules-editor-disclosure').click(); await settle() })
    await settle()
    const exactDraft = `${advancedSource}\n# Keep my draft after an uncertain write.`
    fill('rules-source', exactDraft)
    await act(async () => { testElement('rules-validate').click(); await settle() })
    expect(host.querySelector('[data-testid="rules-validation-summary"]')?.textContent).toContain('Valid draft')

    await act(async () => { testElement('rules-save').click(); await settle() })
    expect(host.querySelector('[aria-labelledby="rules-save-confirmation"]')).not.toBeNull()
    expect(vi.mocked(guiRulesManage).mock.calls.filter(([operation]) => operation === 'rules.save')).toHaveLength(0)
    vi.mocked(guiRulesManage).mockRejectedValueOnce(new Error('transport disconnected'))
    await act(async () => { [...host.querySelectorAll('button')].find((button) => button.textContent?.trim() === 'Save exact validated draft')!.click(); await settle() })
    expect(vi.mocked(guiRulesManage).mock.calls.filter(([operation]) => operation === 'rules.save')).toHaveLength(1)
    expect(testElement<HTMLTextAreaElement>('rules-source').value).toBe(exactDraft)
    expect(host.querySelector('[data-testid="rules-verify-save"]')).not.toBeNull()
    expect(onRulesChanged).not.toHaveBeenCalled()

    await act(async () => { testElement('rules-verify-save').click(); await settle() })
    expect(vi.mocked(guiRulesManage).mock.calls.filter(([operation]) => operation === 'rules.save')).toHaveLength(1)
    expect(testElement<HTMLTextAreaElement>('rules-source').value).toBe(exactDraft)
    expect(testElement('rules-remote-source').textContent).toContain(advancedSource)

    documentByScope.global = { source: '# Remote version changed.', revision: 'global-r2', scope: 'global', project_id: null }
    await act(async () => { testElement('rules-validate').click(); await settle() })
    vi.mocked(guiRulesManage).mockResolvedValueOnce({ source: exactDraft, revision: 'global-r3', scope: 'global', project_id: null } as never)
    await act(async () => { testElement('rules-save').click(); await settle() })
    await act(async () => { [...host.querySelectorAll('button')].find((button) => button.textContent?.trim() === 'Save exact validated draft')!.click(); await settle() })
    const saveCalls = vi.mocked(guiRulesManage).mock.calls.filter(([operation]) => operation === 'rules.save')
    expect(saveCalls).toHaveLength(2)
    expect(saveCalls[1]).toEqual(['rules.save', { source: exactDraft, if_revision: 'global-r1' }])
  })

  it('retains the exact draft and validation after RULES_BUSY, with only a manual retry available', async () => {
    const onRulesChanged = vi.fn(async () => undefined)
    await act(async () => {
      host = document.createElement('div')
      document.body.append(host)
      root = createRoot(host)
      root.render(<RulesEditor language="ja" currentAccountId="account-a" beginOperation={() => true} endOperation={() => undefined} onRulesChanged={onRulesChanged} />)
      await settle()
    })
    await act(async () => { testElement('rules-editor-disclosure').click(); await settle() })
    await settle()

    const exactDraft = `${advancedSource}\n# Busy save must keep this exact draft.`
    fill('rules-source', exactDraft)
    await act(async () => { testElement('rules-validate').click(); await settle() })
    expect(host.querySelector('[data-testid="rules-validation-summary"]')?.textContent).toContain('Valid draft')
    await act(async () => { testElement('rules-save').click(); await settle() })
    vi.mocked(guiRulesManage).mockRejectedValueOnce({ code: 'RULES_BUSY', message: 'the file is open' })
    await act(async () => {
      [...host.querySelectorAll('button')].find((button) => button.textContent?.trim() === '検証済みの内容を保存')!.click()
      await settle()
    })

    expect(testElement<HTMLTextAreaElement>('rules-source').value).toBe(exactDraft)
    expect(host.querySelector('[data-testid="rules-validation-summary"]')?.textContent).toContain('Valid draft')
    expect(host.querySelector('[role="alert"]')?.textContent).toContain('他の編集を閉じてから、保存を手動で再試行')
    expect(vi.mocked(guiRulesManage).mock.calls.filter(([operation]) => operation === 'rules.save')).toHaveLength(1)
    expect(onRulesChanged).not.toHaveBeenCalled()

    // The user can explicitly review a retry; the failed write is never repeated automatically.
    expect(testElement<HTMLButtonElement>('rules-save').disabled).toBe(false)
    await act(async () => { testElement('rules-save').click(); await settle() })
    expect(host.querySelector('[aria-labelledby="rules-save-confirmation"]')).not.toBeNull()
    expect(vi.mocked(guiRulesManage).mock.calls.filter(([operation]) => operation === 'rules.save')).toHaveLength(1)
    await act(async () => { [...host.querySelectorAll('button')].find((button) => button.textContent?.trim() === 'キャンセル')!.click(); await settle() })
  })

  it('keeps a rejected stale revision as a draft until the user reviews and confirms reload', async () => {
    await render()
    const draft = `${advancedSource}\n# Unsaved revision-conflict draft.`
    fill('rules-source', draft)
    await act(async () => { testElement('rules-validate').click(); await settle() })
    await act(async () => { testElement('rules-save').click(); await settle() })
    vi.mocked(guiRulesManage).mockRejectedValueOnce({ code: 'RULES_CHANGED', message: 'recovery-ref-17' })
    await act(async () => { [...host.querySelectorAll('button')].find((button) => button.textContent?.trim() === 'Save exact validated draft')!.click(); await settle() })
    expect(host.textContent).toContain('This Rules document changed elsewhere')
    expect(host.textContent).toContain('recovery-ref-17')
    expect(testElement<HTMLTextAreaElement>('rules-source').value).toBe(draft)
    await act(async () => { testElement('rules-reload-conflict').click(); await settle() })
    expect(host.querySelector('[aria-labelledby="rules-reload-title"]')).not.toBeNull()
    expect(testElement<HTMLTextAreaElement>('rules-source').value).toBe(draft)
    await act(async () => { [...host.querySelectorAll('button')].find((button) => button.textContent?.trim() === 'Discard draft and reload')!.click(); await settle() })
    expect(testElement<HTMLTextAreaElement>('rules-source').value).toBe(advancedSource)
  })
})
