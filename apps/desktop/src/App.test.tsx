// @vitest-environment jsdom
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import App from './App'
import { authenticate, cancelPending, getSnapshot, guiAccountManage, guiAuthorizationManage, guiCreationStatus, guiRead, guiRuntimeStart, guiRuntimeStatus, guiRuntimeStop, guiStartupSet, guiStartupStatus, logout, recoverAccount, revealRecoveryCode, retryPending } from './bridge'
import type { GuiSnapshot, StartupStatus } from './types'
import { guiSetPassword } from './bridge'

vi.mock('./bridge', () => ({
  guiRestoreStatus:vi.fn(async()=>null),guiPickRestoreBackup:vi.fn(),guiPrepareRestore:vi.fn(),guiClearRestore:vi.fn(),
  guiGoogleLink:vi.fn(), guiGoogleUnlink:vi.fn(), guiGoogleRecover:vi.fn(),
  guiSetPassword: vi.fn(),
  authenticate: vi.fn(),
  cancelPending: vi.fn(),
  getSnapshot: vi.fn(),
  guiAccountManage: vi.fn(),
  guiAuthorizationManage: vi.fn(),
  guiCreationStatus: vi.fn(),
  guiRuntimeStart: vi.fn(),
  guiRuntimeStatus: vi.fn(),
  guiRuntimeStop: vi.fn(),
  guiStartupSet: vi.fn(),
  guiStartupStatus: vi.fn(),
  logout: vi.fn(),
  recoverAccount: vi.fn(),
  revealRecoveryCode: vi.fn(),
  retryPending: vi.fn(),
  guiRead: vi.fn(),
  guiNotificationActivation: vi.fn(async () => ({ state: 'none' })),
  guiNotificationAck: vi.fn(),
  guiSubscribeThread: vi.fn(),
  guiMarkDisplayedPostsRead: vi.fn(),
  guiSetNotificationSettings: vi.fn(),
  guiBackupStatus: vi.fn(async () => ({ settings: { enabled: true, interval_seconds: 86400, retention: 7, destination: null }, default_destination: 'C:\\private\\backups', journal_error: null, pending_stage: null, schedule: { version: 1, last_success_at: null, pending: null, generations: [], prune: [] } })),
  guiStorageInventory: vi.fn(async () => ({ archives: [], recovery_bytes: 0 })),
  guiDeleteBackupArchive: vi.fn(), guiClearRestoreArtifacts: vi.fn(),
  guiSetBackupSettings: vi.fn(async (_id, settings) => settings),
  guiCreateManualBackup:vi.fn(), guiAckManualBackup:vi.fn(),
  guiRetentionSettings:vi.fn(async()=>({history_days:null,deleted_days:null})),guiSetRetentionSettings:vi.fn(),
  guiDisplayDefaults: vi.fn(async () => ({ language: 'en', theme: 'light' })),
  guiDisplaySettings: vi.fn(async () => ({ settings: null })),
  guiSetDisplaySettings: vi.fn(async (_id, settings) => settings),
}))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => vi.fn()) }))
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => ({ setTheme: vi.fn(async () => undefined) }) }))

const loginSnapshot: GuiSnapshot = { setupRequired: false, account: null, pendingIssue: false }
const pendingSnapshot: GuiSnapshot = { setupRequired: true, account: null, pendingIssue: true }
const signedInSnapshot: GuiSnapshot = {
  setupRequired: false,
  account: { id: 'account-1', name: 'Ada', is_admin: true, disabled: false, role_ids: ['role-reviewer'] },
  pendingIssue: false,
}

let host: HTMLDivElement
let root: Root

function setInputValue(input: HTMLInputElement, value: string) {
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set
  setter?.call(input, value)
  input.dispatchEvent(new Event('input', { bubbles: true }))
}

function submitForm() {
  const form = host.querySelector('form')
  if (!form) throw new Error('Authentication form is not rendered')
  form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
}

function submitTestForm(testId: string) {
  const form = host.querySelector<HTMLFormElement>(`[data-testid="${testId}"]`)
  if (!form) throw new Error(`Form missing: ${testId}`)
  form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
}

function setInputByTestId(testId: string, value: string) {
  const input = host.querySelector<HTMLInputElement>(`[data-testid="${testId}"]`)
  if (!input) throw new Error(`Input missing: ${testId}`)
  setInputValue(input, value)
}

function clickButton(label: string) {
  const button = [...host.querySelectorAll('button')].find((item) => item.textContent?.includes(label))
  if (!button) throw new Error(`Button not found: ${label}`)
  button.click()
}

function buttonByTestId(testId: string) {
  const button = host.querySelector<HTMLButtonElement>(`[data-testid="${testId}"]`)
  if (!button) throw new Error(`Button missing: ${testId}`)
  return button
}

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((done) => { resolve = done })
  return { promise, resolve }
}

async function settleReact() {
  await new Promise<void>((resolve) => setTimeout(resolve, 0))
}

async function renderApp(inTauri = true) {
  if (inTauri) Object.defineProperty(window, '__TAURI_INTERNALS__', { value: {}, configurable: true })
  host = document.createElement('div')
  document.body.append(host)
  root = createRoot(host)
  await act(async () => {
    root.render(<App />)
    await settleReact()
  })
}

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(getSnapshot).mockResolvedValue(loginSnapshot)
  vi.mocked(authenticate).mockResolvedValue(signedInSnapshot)
  vi.mocked(retryPending).mockResolvedValue(signedInSnapshot)
  vi.mocked(cancelPending).mockResolvedValue(loginSnapshot)
  vi.mocked(recoverAccount).mockResolvedValue(signedInSnapshot)
  vi.mocked(guiAccountManage).mockResolvedValue(signedInSnapshot.account!)
  vi.mocked(guiAuthorizationManage).mockImplementation(async (_operation, args) => args as never)
  vi.mocked(guiCreationStatus).mockResolvedValue({ pending: false, operation: null, expired: false })
  vi.mocked(guiRuntimeStart).mockResolvedValue({ state: 'running' })
  vi.mocked(guiRuntimeStatus).mockResolvedValue({ state: 'running' })
  vi.mocked(guiRuntimeStop).mockResolvedValue({ state: 'stopped' })
  vi.mocked(guiStartupStatus).mockResolvedValue({ enabled: true, registered: true, configured: true })
  vi.mocked(guiStartupSet).mockImplementation(async (enabled) => ({ enabled, registered: enabled, configured: true }))
  vi.mocked(revealRecoveryCode).mockResolvedValue({ recovery_code: 'TEST-RECOVERY-CODE' })
  vi.mocked(logout).mockResolvedValue(loginSnapshot)
  vi.mocked(guiRead).mockImplementation(async (operation) => {
    if (operation === 'conversation.capabilities') return { board_create: false, boards: [], threads: [] } as never
    if (operation === 'management.capabilities') return { account_list: false, account_get: false, account_create: false, account_disable: false, account_reset: false } as never
    if (operation === 'account.list') return { items: [] } as never
    if (operation === 'board.list' || operation === 'thread.list') return { items: [] } as never
    throw new Error(`Unexpected read operation: ${operation}`)
  })
  Object.defineProperty(globalThis, 'IS_REACT_ACT_ENVIRONMENT', { value: true, configurable: true })
})

afterEach(async () => {
  if (root) await act(async () => root.unmount())
  host?.remove()
  delete (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__
})

it('dismisses cached private settings when remote management rights are revoked', async () => {
  Object.defineProperty(document, 'visibilityState', { value: 'visible', configurable: true })
  vi.mocked(getSnapshot).mockResolvedValue(signedInSnapshot)
  const originalRead = vi.mocked(guiRead).getMockImplementation()!
  let permitted = true
  vi.mocked(guiRead).mockImplementation(async (operation, args) => {
    if (operation === 'management.capabilities') return { account_list: permitted, account_get: permitted, account_create: false, account_disable: false, account_reset: false } as never
    if (operation === 'account.list') return { items: [{ id: 'private-member', name: 'PRIVATE_MANAGEMENT_CANARY', is_admin: false, disabled: false, role_ids: [] }] } as never
    return originalRead(operation, args) as never
  })
  await renderApp()
  await act(async () => { clickButton('Settings'); await settleReact() })
  expect(host.textContent).toContain('PRIVATE_MANAGEMENT_CANARY')
  permitted = false
  await act(async () => { window.dispatchEvent(new Event('focus')); await settleReact() })
  expect(host.querySelector('[aria-label="Settings"][role="dialog"]')).toBeNull()
  expect(host.textContent).not.toContain('PRIVATE_MANAGEMENT_CANARY')
})

describe('authentication interface', () => {
  it('changes the selected account lock with current-password proof and clears secrets', async () => {
    const locked: GuiSnapshot = { ...signedInSnapshot, account: { ...signedInSnapshot.account!, lock_enabled: true } }
    const unlocked: GuiSnapshot = { ...signedInSnapshot, account: { ...signedInSnapshot.account!, lock_enabled: false } }
    vi.mocked(getSnapshot).mockResolvedValue(locked)
    vi.mocked(guiSetPassword).mockResolvedValueOnce(unlocked).mockResolvedValueOnce(locked)
    await renderApp()
    await act(async () => clickButton('Settings'))
    let panel = host.querySelector('.password-lock-settings')!
    const disable = [...panel.querySelectorAll('button')].find(button => button.textContent === 'Disable password lock')!
    await act(async () => disable.click())
    expect(guiSetPassword).not.toHaveBeenCalled()
    await act(async () => setInputValue(panel.querySelector('input')!, 'current-secret'))
    await act(async () => { disable.click(); await settleReact() })
    expect(guiSetPassword).toHaveBeenNthCalledWith(1, 'account-1', 'current-secret', '')
    panel = host.querySelector('.password-lock-settings')!
    expect(panel.textContent).toContain('Password lock is off.')
    const inputs = panel.querySelectorAll('input')
    expect([...inputs].every(input => input.value === '')).toBe(true)
    await act(async () => { setInputValue(inputs[0], 'new-secret'); setInputValue(inputs[1], 'new-secret') })
    await act(async () => { panel.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); await settleReact() })
    expect(guiSetPassword).toHaveBeenNthCalledWith(2, 'account-1', '', 'new-secret')
    expect(host.querySelector('.password-lock-settings')!.textContent).toContain('Password lock is on.')
    expect(host.querySelector('[role="status"].change-saved-toast')?.textContent).toBe('Password lock enabled')
    vi.mocked(guiSetPassword).mockResolvedValueOnce(locked)
    panel = host.querySelector('.password-lock-settings')!
    const changeInputs = panel.querySelectorAll('input')
    await act(async () => { setInputValue(changeInputs[0], 'new-secret'); setInputValue(changeInputs[1], 'changed-secret'); setInputValue(changeInputs[2], 'changed-secret') })
    await act(async () => { panel.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); await settleReact() })
    expect(host.querySelector('[role="status"].change-saved-toast')?.textContent).toBe('Password changed')
    expect([...host.querySelectorAll<HTMLInputElement>('.password-lock-settings input')].every(input => input.value === '')).toBe(true)
  })

  it('skips only the initial password, clears a typed secret, and requires an account name', async () => {
    vi.mocked(getSnapshot).mockResolvedValue({ setupRequired: true, account: null, pendingIssue: false })
    await renderApp()
    expect(host.textContent).toContain('You can set up password locking later.')
    await act(async () => buttonByTestId('setup-skip-password').click())
    expect(authenticate).not.toHaveBeenCalled()
    const operation = deferred<GuiSnapshot>()
    vi.mocked(authenticate).mockReturnValue(operation.promise)
    const inputs = host.querySelectorAll('input')
    await act(async () => { setInputValue(inputs[0], 'Ada'); setInputValue(inputs[1], 'discard-this-password') })
    await act(async () => { buttonByTestId('setup-skip-password').click(); buttonByTestId('setup-skip-password').click() })
    expect(authenticate).toHaveBeenCalledExactlyOnceWith('setup', 'Ada', '')
    expect(inputs[1].value).toBe('')
    await act(async () => { operation.resolve(signedInSnapshot); await settleReact() })
    expect(host.querySelector('[data-testid="setup-skip-password"]')).toBeNull()
  })

  it('clears the password immediately and serializes duplicate submits while busy', async () => {
    const operation = deferred<GuiSnapshot>()
    vi.mocked(authenticate).mockReturnValue(operation.promise)
    await renderApp()

    const inputs = host.querySelectorAll('input')
    await act(async () => {
      setInputValue(inputs[0], 'Ada')
      setInputValue(inputs[1], 'secret')
    })
    await act(async () => {
      submitForm()
      submitForm()
      await settleReact()
    })

    expect(inputs[1].value).toBe('')
    expect(host.querySelector<HTMLButtonElement>('.submit-button')?.disabled).toBe(true)
    expect(authenticate).toHaveBeenCalledTimes(1)
    expect(authenticate).toHaveBeenCalledWith('login', 'Ada', 'secret')

    await act(async () => {
      operation.resolve(signedInSnapshot)
      await operation.promise
    })
    expect(host.querySelector('.workspace-panel')).not.toBeNull()
    expect(host.textContent).toContain('Settings')
    expect(host.textContent).not.toContain('account-1')
  })

  it('refreshes after a wrong-password fault and keeps the login form available', async () => {
    vi.mocked(authenticate).mockRejectedValue({ code: 'AUTHENTICATION_REQUIRED', exit_code: 3, message: 'A valid login is required' })
    await renderApp()

    const inputs = host.querySelectorAll('input')
    await act(async () => {
      setInputValue(inputs[0], 'Ada')
      setInputValue(inputs[1], 'wrong')
    })
    await act(async () => {
      submitForm()
      await settleReact()
    })

    expect(getSnapshot).toHaveBeenCalledTimes(2)
    expect(host.querySelector('form')).not.toBeNull()
    expect(host.querySelector('[role="alert"]')?.textContent).toContain('A valid login is required')
    expect(host.textContent).not.toContain('Can’t reach Swarm right now')
    expect(host.querySelector('.content-wrap')?.classList.contains('has-workspace')).toBe(false)
    expect(host.querySelector('.auth-panel')).not.toBeNull()
  })

  it('preserves the login screen until authentication succeeds, then mounts the compact workspace shell', async () => {
    await renderApp()
    expect(host.querySelector('.auth-panel')).not.toBeNull()
    expect(host.querySelector('.content-wrap')?.classList.contains('has-workspace')).toBe(false)
    expect(host.querySelector('.workspace-panel')).toBeNull()

    const inputs = host.querySelectorAll<HTMLInputElement>('.auth-form input')
    await act(async () => {
      setInputValue(inputs[0], 'Ada')
      setInputValue(inputs[1], 'secret')
      submitForm()
      await settleReact()
    })

    const content = host.querySelector<HTMLElement>('.content-wrap')
    expect(content?.classList.contains('has-workspace')).toBe(true)
    expect(host.querySelector('.auth-panel')).toBeNull()
    expect(host.querySelector('.workspace-panel')).not.toBeNull()
    expect(host.querySelector('.topbar .brand')).toBeNull()
    const workspaceToolbar = host.querySelector<HTMLElement>('.workspace-toolbar')!
    expect(workspaceToolbar).toBeTruthy()
    expect(workspaceToolbar.textContent).toContain('Swarm')
    expect(workspaceToolbar.querySelector('img[src="/icon.png"]')).not.toBeNull()
    expect(host.textContent).not.toContain('Ada')
    expect(host.textContent).not.toContain('account-1')
    expect(host.textContent).not.toContain('role-reviewer')
    expect(host.querySelector('#settings-account-name')).toBeNull()
    expect(host.querySelector('#settings-account-id')).toBeNull()
    expect(host.querySelector('#settings-account-roles')).toBeNull()
    // jsdom confirms the authenticated shell state, not viewport sizing or rendered geometry.
  })

  it('shows account preferences in an accessible Settings dialog and restores focus on close', async () => {
    await renderApp()
    const inputs = host.querySelectorAll<HTMLInputElement>('.auth-form input')
    await act(async () => {
      setInputValue(inputs[0], 'Ada')
      setInputValue(inputs[1], 'secret')
      submitForm()
      await settleReact()
    })
    const opener = [...host.querySelectorAll<HTMLButtonElement>('button')].find((item) => item.textContent?.trim() === 'Settings')!
    expect(opener).toBeTruthy()
    await act(async () => { opener.click() })

    let dialog = host.querySelector<HTMLElement>('[role="dialog"][aria-modal="true"][aria-label="Settings"]')!
    expect(dialog).toBeTruthy()
    expect(dialog.querySelector('.settings-dialog-heading, #settings-title')).toBeNull()
    expect(dialog.firstElementChild?.getAttribute('role')).toBe('tablist')
    expect(dialog.lastElementChild?.tagName).toBe('FOOTER')
    expect(dialog.querySelector('footer .settings-close-button')?.textContent).toBe('Close settings')
    const settingsTabs = [...dialog.querySelectorAll<HTMLButtonElement>('[role="tab"]')]
    expect(settingsTabs.map((tab) => tab.textContent)).toEqual(['Basic settings', 'Bug reports', 'Account', 'Projects', 'Permissions & rules', 'Notifications', 'Data retention', 'Backup', 'Startup & runtime'])
    expect(settingsTabs[0].getAttribute('aria-selected')).toBe('true')
    expect(document.activeElement).toBe(settingsTabs[0])
    expect(dialog.querySelector('[role="tabpanel"]:not([hidden]) button[aria-label="Language"]')).not.toBeNull()
    await act(async () => { settingsTabs[2].click(); await settleReact() })
    expect(dialog.querySelector('#settings-panel-basic')?.hasAttribute('hidden')).toBe(true)
    expect(dialog.querySelector('#settings-panel-account')?.hasAttribute('hidden')).toBe(false)
    settingsTabs[2].focus()
    await act(async () => { settingsTabs[2].dispatchEvent(new KeyboardEvent('keydown', { key: 'Home', bubbles: true, cancelable: true })); await settleReact() })
    expect(settingsTabs[0].getAttribute('aria-selected')).toBe('true')
    expect(document.activeElement).toBe(settingsTabs[0])
    expect(dialog.querySelector('#settings-account-name')?.textContent).toContain('Ada')
    expect(dialog.querySelector('#settings-account-status')).not.toBeNull()
    expect(dialog.querySelector('#settings-account-id')?.textContent).toContain('account-1')
    expect(dialog.querySelector('#settings-account-roles')?.textContent).toContain('role-reviewer')
    expect(dialog.querySelector('.settings-note')?.textContent).toContain('saved per account')
    const background = host.querySelector<HTMLElement>('#app-content')!
    expect(background.hasAttribute('inert')).toBe(true)
    expect(background.getAttribute('aria-hidden')).toBe('true')
    expect(dialog.contains(document.activeElement)).toBe(true)
    expect([...dialog.querySelectorAll('button')].some((item) => item.getAttribute('aria-label') === 'Language')).toBe(true)
    expect([...dialog.querySelectorAll('button')].some((item) => item.getAttribute('aria-label') === 'Appearance')).toBe(true)
    const focusable = [...dialog.querySelectorAll<HTMLElement>('button:not([disabled]), [href], input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])')].filter((element) => !element.closest('[hidden]') && element.tabIndex >= 0)
    const firstFocusable = focusable[0]
    focusable[focusable.length - 1].focus()
    dialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', bubbles: true, cancelable: true }))
    expect(document.activeElement).toBe(firstFocusable)

    await act(async () => { dialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })) })
    expect(host.querySelector('[role="dialog"]')).toBeNull()
    expect(document.activeElement).toBe(opener)
    expect(background.hasAttribute('inert')).toBe(false)
    expect(background.hasAttribute('aria-hidden')).toBe(false)

    await act(async () => { opener.click() })
    dialog = host.querySelector<HTMLElement>('[role="dialog"][aria-modal="true"]')!
    const close = [...dialog.querySelectorAll<HTMLButtonElement>('button')].find((item) => item.textContent?.trim() === 'Close settings')
    expect(close).toBeTruthy()
    await act(async () => { close?.click() })
    expect(host.querySelector('[role="dialog"]')).toBeNull()
    expect(document.activeElement).toBe(opener)
  })

  it('confirms complete stop separately from closing Settings and explicitly resumes with a workspace refresh', async () => {
    vi.mocked(getSnapshot).mockResolvedValue(signedInSnapshot)
    await renderApp()
    await act(async () => { clickButton('Settings'); await settleReact() })
    expect(guiRuntimeStatus).toHaveBeenCalledTimes(2)
    expect(host.querySelector('[data-testid="workspace-service-status"]')?.textContent).toContain('Running')
    expect(guiRuntimeStart).not.toHaveBeenCalled()
    expect(host.querySelector('[data-testid="runtime-state"]')?.textContent).toContain('Running')

    await act(async () => { buttonByTestId('runtime-stop-open').click(); await settleReact() })
    expect(host.querySelector('[data-testid="runtime-stop-confirmation"]')).not.toBeNull()
    expect(host.textContent).toContain('Closing this window alone leaves Swarm running')
    await act(async () => { buttonByTestId('runtime-stop-cancel').click(); await settleReact() })
    expect(guiRuntimeStop).not.toHaveBeenCalled()

    await act(async () => { buttonByTestId('runtime-stop-open').click(); await settleReact() })
    await act(async () => { buttonByTestId('runtime-stop-confirm').click(); await settleReact() })
    expect(guiRuntimeStop).toHaveBeenCalledTimes(1)
    expect(host.querySelector('[data-testid="runtime-state"]')?.textContent).toContain('Stopped by your request')
    expect(host.querySelector('[data-testid="workspace-service-status"]')?.textContent).toContain('Stopped')
    expect(host.querySelector('[data-testid="runtime-stop-confirmation"]')).toBeNull()

    await act(async () => { clickButton('Close settings'); await settleReact() })
    expect(host.querySelector('[data-testid="runtime-stopped-banner"]')).not.toBeNull()
    expect(host.querySelector('#app-content')?.hasAttribute('inert')).toBe(true)
    expect(guiRuntimeStart).not.toHaveBeenCalled()

    await act(async () => { buttonByTestId('runtime-banner-start').click(); await settleReact() })
    expect(guiRuntimeStart).toHaveBeenCalledTimes(1)
    expect(getSnapshot).toHaveBeenCalledTimes(2)
    expect(host.querySelector('[data-testid="runtime-stopped-banner"]')).toBeNull()
    expect(host.querySelector('#app-content')?.hasAttribute('inert')).toBe(false)
    expect(host.querySelector('.workspace-panel')).not.toBeNull()
  })

  it('surfaces runtime status read failures without treating them as stopped or starting automatically', async () => {
    vi.mocked(getSnapshot).mockResolvedValue(signedInSnapshot)
    vi.mocked(guiRuntimeStatus).mockRejectedValue(new Error('status pipe unavailable'))
    await renderApp()
    await act(async () => { clickButton('Settings'); await settleReact() })
    expect(host.querySelector('[data-testid="runtime-error"]')?.textContent).toContain('Could not check the runtime status.')
    expect(host.querySelector('[data-testid="runtime-stopped-banner"]')).toBeNull()
    expect(guiRuntimeStart).not.toHaveBeenCalled()
    expect(guiRuntimeStop).not.toHaveBeenCalled()
  })

  it('marks the workspace unavailable and offers explicit start when status reports not running', async () => {
    vi.mocked(getSnapshot).mockResolvedValue(signedInSnapshot)
    vi.mocked(guiRuntimeStatus).mockResolvedValue({ state: 'not_running' })
    await renderApp()
    await act(async () => { clickButton('Settings'); await settleReact() })
    await act(async () => { clickButton('Close settings'); await settleReact() })

    expect(host.querySelector('#app-content')?.hasAttribute('inert')).toBe(true)
    expect(host.querySelector('[data-testid="runtime-stopped-banner"]')?.textContent).toContain('Swarm is not running')
    expect(host.querySelector('[data-testid="runtime-banner-start"]')).not.toBeNull()
    expect(guiRuntimeStart).not.toHaveBeenCalled()
  })

  it('preserves a runtime start failure after refreshing status', async () => {
    vi.mocked(getSnapshot).mockResolvedValue(signedInSnapshot)
    vi.mocked(guiRuntimeStatus)
      .mockResolvedValueOnce({ state: 'running' })
      .mockResolvedValueOnce({ state: 'not_running' })
      .mockResolvedValueOnce({ state: 'not_running' })
    vi.mocked(guiRuntimeStart).mockRejectedValueOnce(new Error('launch failed'))
    await renderApp()
    await act(async () => { clickButton('Settings'); await settleReact() })
    await act(async () => { buttonByTestId('runtime-start').click(); await settleReact() })

    expect(host.querySelector('[data-testid="runtime-error"]')?.textContent).toContain('Could not start the background runtime')
    expect(host.querySelector('[data-testid="runtime-state"]')?.textContent).toContain('Not running')
    expect(host.querySelector('.workspace-panel')).not.toBeNull()
  })

  it('fails closed after a started runtime cannot restore the workspace and keeps explicit recovery available', async () => {
    vi.mocked(getSnapshot)
      .mockResolvedValueOnce(signedInSnapshot)
      .mockRejectedValueOnce(new Error('snapshot transport failed'))
      .mockResolvedValueOnce(signedInSnapshot)
    vi.mocked(guiRuntimeStatus)
      .mockResolvedValueOnce({ state: 'running' })
      .mockResolvedValueOnce({ state: 'not_running' })
      .mockResolvedValueOnce({ state: 'running' })
    await renderApp()
    await act(async () => { clickButton('Settings'); await settleReact() })
    await act(async () => { buttonByTestId('runtime-start').click(); await settleReact() })

    expect(host.querySelector('.workspace-panel')).toBeNull()
    expect(host.querySelector('[data-testid="connection-runtime-start"]')).not.toBeNull()
    expect(host.querySelector('[data-testid="runtime-error"]')?.textContent).toContain('workspace did not refresh')

    await act(async () => { buttonByTestId('connection-runtime-start').click(); await settleReact() })
    expect(guiRuntimeStart).toHaveBeenCalledTimes(2)
    expect(host.querySelector('.workspace-panel')).not.toBeNull()
    expect(host.querySelector('[data-testid="connection-runtime-start"]')).toBeNull()
  })

  it('ignores an older Settings status response after closing and reopening the dialog', async () => {
    vi.mocked(getSnapshot).mockResolvedValue(signedInSnapshot)
    const earlier = deferred<{ state: 'running' | 'stopped' | 'not_running' }>()
    vi.mocked(guiRuntimeStatus)
      .mockResolvedValueOnce({ state: 'running' })
      .mockReturnValueOnce(earlier.promise)
      .mockResolvedValueOnce({ state: 'stopped' })
    await renderApp()
    await act(async () => { clickButton('Settings'); await settleReact() })
    await act(async () => { clickButton('Close settings'); await settleReact() })
    await act(async () => { clickButton('Settings'); await settleReact() })
    expect(host.querySelector('[data-testid="runtime-state"]')?.textContent).toContain('Stopped by your request')

    await act(async () => { earlier.resolve({ state: 'running' }); await settleReact() })
    expect(host.querySelector('[data-testid="runtime-state"]')?.textContent).toContain('Stopped by your request')
  })

  it('shows a missing Windows registration and explicitly applies the desired startup setting', async () => {
    vi.mocked(getSnapshot).mockResolvedValue(signedInSnapshot)
    vi.mocked(guiStartupStatus).mockResolvedValueOnce({ enabled: true, registered: false, configured: false })
    await renderApp()
    await act(async () => { clickButton('Settings'); await settleReact() })

    const toggle = host.querySelector<HTMLInputElement>('[data-testid="windows-startup-toggle"]')!
    expect(toggle.checked).toBe(true)
    expect(toggle.disabled).toBe(false)
    expect(host.querySelector('[data-testid="windows-startup-warning"]')?.textContent).toContain('registration is missing')
    expect(host.querySelector('[data-testid="windows-startup-state"]')?.textContent).toContain('default enabled setting')
    await act(async () => { buttonByTestId('windows-startup-apply').click(); await settleReact() })

    expect(guiStartupSet).toHaveBeenCalledWith(true)
    expect(host.querySelector('[data-testid="windows-startup-warning"]')).toBeNull()
    expect(host.querySelector('[data-testid="windows-startup-error"]')).toBeNull()
  })

  it('keeps startup changes disabled while the runtime is not running', async () => {
    vi.mocked(getSnapshot).mockResolvedValue(signedInSnapshot)
    vi.mocked(guiRuntimeStatus).mockResolvedValue({ state: 'stopped' })
    vi.mocked(guiStartupStatus).mockResolvedValue({ enabled: true, registered: false, configured: true })
    await renderApp()
    await act(async () => { clickButton('Settings'); await settleReact() })

    expect(host.querySelector<HTMLInputElement>('[data-testid="windows-startup-toggle"]')?.disabled).toBe(true)
    expect(host.querySelector<HTMLButtonElement>('[data-testid="windows-startup-apply"]')?.disabled).toBe(true)
    expect(host.querySelector<HTMLButtonElement>('[data-testid="windows-startup-refresh"]')?.disabled).toBe(false)
    expect(guiStartupSet).not.toHaveBeenCalled()
  })

  it('preserves a failed registration update and retries the same desired value only after an explicit click', async () => {
    vi.mocked(getSnapshot).mockResolvedValue(signedInSnapshot)
    vi.mocked(guiStartupStatus)
      .mockResolvedValueOnce({ enabled: true, registered: true, configured: true })
      .mockResolvedValueOnce({ enabled: false, registered: true, configured: true })
    vi.mocked(guiStartupSet)
      .mockRejectedValueOnce(new Error('registry update failed'))
      .mockResolvedValueOnce({ enabled: false, registered: false, configured: true })
    await renderApp()
    await act(async () => { clickButton('Settings'); await settleReact() })
    const toggle = host.querySelector<HTMLInputElement>('[data-testid="windows-startup-toggle"]')!
    await act(async () => { toggle.click(); await settleReact() })

    expect(guiStartupSet).toHaveBeenCalledTimes(1)
    expect(guiStartupSet).toHaveBeenNthCalledWith(1, false)
    expect(toggle.checked).toBe(false)
    expect(host.querySelector('[data-testid="windows-startup-error"]')?.textContent).toContain('Check the displayed setting and retry')
    expect(host.querySelector('[data-testid="windows-startup-warning"]')?.textContent).toContain('registration is still present')

    await act(async () => { buttonByTestId('windows-startup-apply').click(); await settleReact() })
    expect(guiStartupSet).toHaveBeenCalledTimes(2)
    expect(guiStartupSet).toHaveBeenNthCalledWith(2, false)
    expect(host.querySelector('[data-testid="windows-startup-warning"]')).toBeNull()
    expect(host.querySelector('[data-testid="windows-startup-error"]')).toBeNull()
  })

  it('clears stale startup status when both the write and follow-up read fail, then permits an explicit retry', async () => {
    vi.mocked(getSnapshot).mockResolvedValue(signedInSnapshot)
    vi.mocked(guiStartupStatus)
      .mockResolvedValueOnce({ enabled: true, registered: true, configured: true })
      .mockRejectedValueOnce(new Error('status unavailable'))
    vi.mocked(guiStartupSet)
      .mockRejectedValueOnce(new Error('registry update failed'))
      .mockResolvedValueOnce({ enabled: false, registered: false, configured: true })
    await renderApp()
    await act(async () => { clickButton('Settings'); await settleReact() })

    const toggle = host.querySelector<HTMLInputElement>('[data-testid="windows-startup-toggle"]')!
    await act(async () => { toggle.click(); await settleReact() })
    expect(host.querySelector('[data-testid="windows-startup-state"]')?.textContent).toContain('Could not check the Windows startup setting')
    expect(host.querySelector('[data-testid="windows-startup-warning"]')).toBeNull()
    expect(host.querySelector('[data-testid="windows-startup-error"]')?.textContent).toContain('Check the displayed setting and retry')
    expect(toggle.disabled).toBe(true)
    expect(host.querySelector<HTMLButtonElement>('[data-testid="windows-startup-apply"]')?.disabled).toBe(false)

    await act(async () => { buttonByTestId('windows-startup-apply').click(); await settleReact() })
    expect(guiStartupSet).toHaveBeenNthCalledWith(2, false)
    expect(host.querySelector<HTMLInputElement>('[data-testid="windows-startup-toggle"]')?.disabled).toBe(false)
    expect(host.querySelector('[data-testid="windows-startup-state"]')?.textContent).toContain('Disabled for Windows sign-in')
    expect(host.querySelector('[data-testid="windows-startup-error"]')).toBeNull()
  })

  it('ignores a stale Windows startup status response after Settings scope changes', async () => {
    vi.mocked(getSnapshot).mockResolvedValue(signedInSnapshot)
    const earlier = deferred<StartupStatus>()
    vi.mocked(guiStartupStatus)
      .mockReturnValueOnce(earlier.promise)
      .mockResolvedValueOnce({ enabled: false, registered: false, configured: true })
    await renderApp()
    await act(async () => { clickButton('Settings'); await settleReact() })
    await act(async () => { clickButton('Close settings'); await settleReact() })
    await act(async () => { clickButton('Settings'); await settleReact() })
    expect(host.querySelector<HTMLInputElement>('[data-testid="windows-startup-toggle"]')?.checked).toBe(false)

    await act(async () => { earlier.resolve({ enabled: true, registered: true, configured: true }); await settleReact() })
    expect(host.querySelector<HTMLInputElement>('[data-testid="windows-startup-toggle"]')?.checked).toBe(false)
  })

  it('discards a Windows startup status response from the previous account after a successful switch', async () => {
    const nextAccount: GuiSnapshot = {
      setupRequired: false,
      account: { id: 'account-2', name: 'Rin', is_admin: false, disabled: false, role_ids: [] },
      pendingIssue: false,
    }
    vi.mocked(getSnapshot).mockResolvedValue(signedInSnapshot)
    vi.mocked(authenticate).mockResolvedValueOnce(nextAccount)
    const previousAccountStatus = deferred<StartupStatus>()
    vi.mocked(guiStartupStatus)
      .mockReturnValueOnce(previousAccountStatus.promise)
      .mockResolvedValueOnce({ enabled: false, registered: false, configured: true })
    await renderApp()
    await act(async () => { clickButton('Settings'); await settleReact() })
    await act(async () => { buttonByTestId('account-switch-toggle').click(); await settleReact() })
    setInputByTestId('account-switch-name', 'Rin')
    setInputByTestId('account-switch-password', 'switch-password')
    await act(async () => { submitTestForm('account-switch-form'); await settleReact() })
    expect(host.querySelector<HTMLInputElement>('[data-testid="windows-startup-toggle"]')?.checked).toBe(false)

    await act(async () => { previousAccountStatus.resolve({ enabled: true, registered: true, configured: true }); await settleReact() })
    expect(host.querySelector<HTMLInputElement>('[data-testid="windows-startup-toggle"]')?.checked).toBe(false)
    expect(host.textContent).not.toContain('switch-password')
  })

  it('keeps pending setup credentials hidden and retries the saved request', async () => {
    vi.mocked(getSnapshot).mockResolvedValue(pendingSnapshot)
    await renderApp()

    expect(host.textContent).toContain('Authentication is waiting to finish')
    expect(host.querySelector('form')).toBeNull()
    await act(async () => {
      clickButton('Retry authentication')
      await settleReact()
    })

    expect(retryPending).toHaveBeenCalledTimes(1)
    expect(authenticate).not.toHaveBeenCalled()
    expect(host.querySelector('.workspace-panel')).not.toBeNull()
    expect(host.textContent).toContain('Settings')
    expect(host.textContent).not.toContain('account-1')
  })

  it('keeps the selected Thread after a failed account switch, then remounts a clean workspace after success', async () => {
    const nextAccount: GuiSnapshot = {
      setupRequired: false,
      account: { id: 'account-2', name: 'Rin', is_admin: false, disabled: false, role_ids: [] },
      pendingIssue: false,
    }
    const board = { id: 'board-a', name: 'Engineering', description: '', created_at: '2026-10-01T00:00:00Z' }
    const thread = { id: 'thread-a', board_id: board.id, name: 'Release review', description: '', created_at: '2026-10-01T00:00:00Z' }
    vi.mocked(getSnapshot).mockResolvedValue(signedInSnapshot)
    vi.mocked(authenticate).mockRejectedValueOnce(new Error('transport interrupted')).mockResolvedValueOnce(nextAccount)
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'conversation.capabilities') return { board_create: false, boards: [{ id: board.id, thread_create: false, board_delete: false }], threads: [{ id: thread.id, post_create: false, response_create: false, post_edit: false, post_history: false, thread_delete: false, post_delete: false, post_restore: false }] } as never
      if (operation === 'conversation.deleted') return { boards: [], threads: [] } as never
      if (operation === 'conversation.pins') return { items: [] } as never
      if (operation === 'conversation.activity') return { items: [], next_before_id: null } as never
      if (operation === 'management.capabilities') return { account_list: false, account_get: false, account_create: false, account_disable: false, account_reset: false } as never
      if (operation === 'account.list') return { items: [] } as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: [thread] } as never
      if (operation === 'thread.get') return thread as never
      if (operation === 'post.page') return { items: [{ id: 'post-1', thread_id: thread.id, no: 1, body: 'Selected state', author: { id: 'a', model_name: 'Model', display_id: 'A1' }, created_at: '2026-10-01T00:00:00Z', updated_at: '2026-10-01T00:00:00Z', revision: 1, reply_to: null, edited_by: null, editor_display_id: null, editor_model_name: null, deleted: false, deleted_at: null, response_count: 0 }], page: 1, page_size: 50, total_posts: 1, total_pages: 1 } as never
      if (operation === 'response.list' || operation === 'post.history') return { items: [], next_before_no: null } as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })
    await renderApp()
    await act(async () => { clickButton('Release review'); await settleReact() })
    expect(host.querySelector('.thread-detail[aria-label="Release review"]')).not.toBeNull()
    await act(async () => { clickButton('Settings'); await settleReact() })
    await act(async () => { host.querySelector<HTMLButtonElement>('[data-testid="account-switch-toggle"]')?.click(); await settleReact() })
    setInputByTestId('account-switch-name', 'Rin')
    setInputByTestId('account-switch-password', 'temporary-switch-secret')
    await act(async () => { submitTestForm('account-switch-form'); await settleReact() })

    expect(getSnapshot).toHaveBeenCalledTimes(2)
    expect(host.querySelector('.thread-detail[aria-label="Release review"]')).not.toBeNull()
    expect(host.querySelector('[data-testid="account-switch-error"]')?.textContent).toContain('Could not switch accounts')
    expect(host.textContent).not.toContain('temporary-switch-secret')
    expect(authenticate).toHaveBeenNthCalledWith(1, 'login', 'Rin', 'temporary-switch-secret')

    setInputByTestId('account-switch-name', 'Rin')
    setInputByTestId('account-switch-password', 'successful-switch-secret')
    await act(async () => { submitTestForm('account-switch-form'); await settleReact() })
    expect(host.querySelector('#settings-account-id')?.textContent).toContain('account-2')
    expect(host.querySelector('.thread-detail[aria-label="Release review"]')).toBeNull()
    expect(host.querySelector('[data-testid="thread-empty-state"]')).not.toBeNull()
    expect(authenticate).toHaveBeenCalledTimes(2)
    expect(host.textContent).not.toContain('successful-switch-secret')
  })

  it('rechecks the same-account Workspace and management rights after an authorization change', async () => {
    const memberSnapshot: GuiSnapshot = {
      setupRequired: false,
      account: { id: 'member-1', name: 'Rin', is_admin: false, disabled: false, role_ids: [] },
      pendingIssue: false,
    }
    const board = { id: 'board-a', name: 'Engineering', description: '', created_at: '2026-10-01T00:00:00Z' }
    const thread = { id: 'thread-a', board_id: board.id, name: 'Restricted review', description: '', created_at: '2026-10-01T00:00:00Z' }
    const post = { id: 'post-1', thread_id: thread.id, no: 1, body: 'Private discussion', author: { id: 'author-1', model_name: 'Model', display_id: 'M1' }, created_at: '2026-10-01T00:00:00Z', updated_at: '2026-10-01T00:00:00Z', revision: 1, reply_to: null, edited_by: null, editor_display_id: null, editor_model_name: null, deleted: false, deleted_at: null, response_count: 0 }
    const account = memberSnapshot.account!
    let revoked = false
    let accountListReads = 0
    vi.mocked(getSnapshot).mockResolvedValue(memberSnapshot)
    vi.mocked(guiAuthorizationManage).mockImplementation(async (operation, args) => {
      expect(operation).toBe('permission.set')
      revoked = true
      return args as never
    })
    vi.mocked(guiRead).mockImplementation(async (operation) => {
      if (operation === 'conversation.capabilities') return { board_create: false, boards: [{ id: board.id, thread_create: false, board_delete: false }], threads: revoked ? [] : [{ id: thread.id, post_create: true, response_create: false, post_edit: false, post_history: false, thread_delete: false, post_delete: false, post_restore: false }] } as never
      if (operation === 'authorization.capabilities') return { role_create: false, role_list: false, role_assign: false, permission_set: true, permission_list: true, operations: [{ operation: 'post.create', scopes: ['thread'] }] } as never
      if (operation === 'management.capabilities') return { account_list: true, account_get: false, account_create: false, account_disable: false, account_reset: false } as never
      if (operation === 'account.list') { accountListReads += 1; return { items: [account] } as never }
      if (operation === 'permission.list') return { subject_type: 'account', subject_id: account.id, items: [] } as never
      if (operation === 'conversation.deleted') return { boards: [], threads: [] } as never
      if (operation === 'conversation.pins') return { items: [] } as never
      if (operation === 'conversation.activity') return { items: [], next_before_id: null } as never
      if (operation === 'board.list') return { items: [board] } as never
      if (operation === 'thread.list') return { items: revoked ? [] : [thread] } as never
      if (operation === 'thread.get') return thread as never
      if (operation === 'post.page') return { items: [post], page: 1, page_size: 50, total_posts: 1, total_pages: 1 } as never
      if (operation === 'response.list' || operation === 'post.history') return { items: [], next_before_no: null } as never
      throw new Error(`Unexpected read operation: ${operation}`)
    })

    await renderApp()
    await act(async () => { clickButton('Restricted review'); await settleReact() })
    expect(host.querySelector('.thread-detail[aria-label="Restricted review"]')?.textContent).toContain('Private discussion')
    await act(async () => { clickButton('Settings'); await settleReact() })
    const initialAccountListReads = accountListReads
    await act(async () => { buttonByTestId('authorization-scope-thread').click(); await settleReact() })
    setInputByTestId('authorization-target-id', thread.id)
    await act(async () => { host.querySelector<HTMLInputElement>('[data-testid="authorization-decision-post.create-deny"]')!.click(); await settleReact() })
    await act(async () => { submitTestForm('authorization-permission-form'); await settleReact() })
    expect(host.querySelector('[data-testid="authorization-confirmation"]')).not.toBeNull()
    await act(async () => { buttonByTestId('authorization-confirm').click(); await settleReact() })

    expect(guiAuthorizationManage).toHaveBeenCalledWith('permission.set', { subject_type: 'account', subject_id: account.id, operation: 'post.create', scope: 'thread', target_id: thread.id, decision: 'deny' })
    expect(host.querySelector('#settings-account-id')?.textContent).toContain(account.id)
    expect(host.querySelector('.thread-detail[aria-label="Restricted review"]')).toBeNull()
    expect(host.querySelector('[data-testid="thread-empty-state"]')).not.toBeNull()
    expect(accountListReads).toBeGreaterThan(initialAccountListReads)
    expect(guiRead).toHaveBeenCalledWith('conversation.capabilities')
    expect(guiRead).toHaveBeenCalledWith('thread.list', { limit: 200 })
  })

  it('clears recovery credentials immediately, serializes duplicate submissions, and applies successful recovery', async () => {
    const operation = deferred<GuiSnapshot>()
    vi.mocked(recoverAccount).mockReturnValue(operation.promise)
    await renderApp()
    await act(async () => { clickButton('日本語') })
    expect(host.textContent).toContain('おかえりなさい')
    await act(async () => { buttonByTestId('admin-recovery-toggle').click(); await settleReact() })
    expect(host.textContent).toContain('復旧すると管理者パスワードを変更')
    setInputByTestId('admin-recovery-name', 'Admin Ada')
    setInputByTestId('admin-recovery-code', 'private-recovery-code')
    setInputByTestId('admin-recovery-password', 'private-new-password')
    await act(async () => { submitTestForm('admin-recovery-form'); submitTestForm('admin-recovery-form'); await settleReact() })

    expect(host.querySelector<HTMLInputElement>('[data-testid="admin-recovery-code"]')?.value).toBe('')
    expect(host.querySelector<HTMLInputElement>('[data-testid="admin-recovery-password"]')?.value).toBe('')
    expect(recoverAccount).toHaveBeenCalledTimes(1)
    expect(recoverAccount).toHaveBeenCalledWith('Admin Ada', 'private-recovery-code', 'private-new-password')
    expect(authenticate).not.toHaveBeenCalled()
    expect(host.textContent).not.toContain('private-recovery-code')
    expect(host.textContent).not.toContain('private-new-password')
    expect(host.querySelector('[data-testid="admin-recovery-submit"]')?.hasAttribute('disabled')).toBe(true)

    await act(async () => { operation.resolve(signedInSnapshot); await operation.promise; await settleReact() })
    expect(host.querySelector('.workspace-panel')).not.toBeNull()
    expect(host.querySelector('[data-testid="admin-recovery-form"]')).toBeNull()
    expect(host.textContent).not.toContain('private-recovery-code')
    expect(host.textContent).not.toContain('private-new-password')
  })

  it('cancels recovery locally and clears the code and new password before the form closes', async () => {
    await renderApp()
    await act(async () => { buttonByTestId('admin-recovery-toggle').click(); await settleReact() })
    setInputByTestId('admin-recovery-name', 'Admin Ada')
    setInputByTestId('admin-recovery-code', 'cancelled-code')
    setInputByTestId('admin-recovery-password', 'cancelled-password')
    await act(async () => { buttonByTestId('admin-recovery-cancel').click(); await settleReact() })

    expect(host.querySelector('[data-testid="admin-recovery-form"]')).toBeNull()
    expect(host.textContent).not.toContain('cancelled-code')
    expect(host.textContent).not.toContain('cancelled-password')
    expect(recoverAccount).not.toHaveBeenCalled()
    await act(async () => { buttonByTestId('admin-recovery-toggle').click(); await settleReact() })
    expect(host.querySelector<HTMLInputElement>('[data-testid="admin-recovery-code"]')?.value).toBe('')
    expect(host.querySelector<HTMLInputElement>('[data-testid="admin-recovery-password"]')?.value).toBe('')
  })

  it('routes a failed account switch to the pending panel if the refreshed snapshot has a pending request', async () => {
    vi.mocked(getSnapshot).mockResolvedValueOnce(signedInSnapshot).mockResolvedValueOnce(pendingSnapshot)
    vi.mocked(authenticate).mockRejectedValueOnce(new Error('switch response lost'))
    await renderApp()
    await act(async () => { clickButton('Settings'); await settleReact() })
    await act(async () => { buttonByTestId('account-switch-toggle').click(); await settleReact() })
    setInputByTestId('account-switch-name', 'Rin')
    setInputByTestId('account-switch-password', 'switch-secret')
    await act(async () => { submitTestForm('account-switch-form'); await settleReact() })

    expect(host.textContent).toContain('Authentication is waiting to finish')
    expect(host.querySelector('[data-testid="pending-discard-toggle"]')).not.toBeNull()
    expect(host.querySelector('[data-testid="account-switch-form"]')).toBeNull()
    expect(host.querySelector('.workspace-panel')).toBeNull()
    expect(retryPending).not.toHaveBeenCalled()
    expect(host.textContent).not.toContain('switch-secret')
  })

  it('routes a failed recovery to pending status when the refreshed snapshot shows the same pending request', async () => {
    vi.mocked(getSnapshot).mockResolvedValueOnce(loginSnapshot).mockResolvedValueOnce(pendingSnapshot)
    vi.mocked(recoverAccount).mockRejectedValueOnce(new Error('recovery response lost'))
    await renderApp()
    await act(async () => { buttonByTestId('admin-recovery-toggle').click(); await settleReact() })
    setInputByTestId('admin-recovery-name', 'Admin Ada')
    setInputByTestId('admin-recovery-code', 'lost-code')
    setInputByTestId('admin-recovery-password', 'lost-password')
    await act(async () => { submitTestForm('admin-recovery-form'); await settleReact() })

    expect(host.textContent).toContain('Authentication is waiting to finish')
    expect(host.querySelector('[data-testid="pending-discard-toggle"]')).not.toBeNull()
    expect(host.querySelector('[data-testid="admin-recovery-form"]')).toBeNull()
    expect(retryPending).not.toHaveBeenCalled()
    expect(host.textContent).not.toContain('lost-code')
    expect(host.textContent).not.toContain('lost-password')
  })

  it('discards a pending request only after confirmation and does not auto-retry or authenticate', async () => {
    vi.mocked(getSnapshot).mockResolvedValue(pendingSnapshot)
    await renderApp()
    await act(async () => { buttonByTestId('pending-discard-toggle').click(); await settleReact() })
    expect(host.textContent).toContain('cannot roll back an action that has already been committed')
    await act(async () => { buttonByTestId('pending-discard-cancel').click(); await settleReact() })
    expect(cancelPending).not.toHaveBeenCalled()
    expect(host.textContent).toContain('Authentication is waiting to finish')
    await act(async () => { buttonByTestId('pending-discard-toggle').click(); await settleReact() })
    await act(async () => { buttonByTestId('pending-discard-confirm').click(); await settleReact() })

    expect(cancelPending).toHaveBeenCalledTimes(1)
    expect(retryPending).not.toHaveBeenCalled()
    expect(authenticate).not.toHaveBeenCalled()
    expect(recoverAccount).not.toHaveBeenCalled()
    expect(host.querySelector('.auth-form')).not.toBeNull()
  })

  it('reports pending-discard errors after refreshing status without retrying the request', async () => {
    vi.mocked(getSnapshot).mockResolvedValue(pendingSnapshot)
    vi.mocked(cancelPending).mockRejectedValueOnce(new Error('discard result unavailable'))
    await renderApp()
    await act(async () => { buttonByTestId('pending-discard-toggle').click(); await settleReact() })
    await act(async () => { buttonByTestId('pending-discard-confirm').click(); await settleReact() })

    expect(host.textContent).toContain('Authentication is waiting to finish')
    expect(host.querySelector('[data-testid="pending-discard-error"]')?.textContent).toContain('Could not discard the saved request')
    expect(cancelPending).toHaveBeenCalledTimes(1)
    expect(retryPending).not.toHaveBeenCalled()
    expect(authenticate).not.toHaveBeenCalled()
  })

  it('shows recovery failure and clears its credentials when recovery cannot be confirmed', async () => {
    vi.mocked(recoverAccount).mockRejectedValueOnce(new Error('recovery unavailable'))
    await renderApp()
    await act(async () => { buttonByTestId('admin-recovery-toggle').click(); await settleReact() })
    setInputByTestId('admin-recovery-name', 'Admin Ada')
    setInputByTestId('admin-recovery-code', 'bad-code')
    setInputByTestId('admin-recovery-password', 'new-secret')
    await act(async () => { submitTestForm('admin-recovery-form'); await settleReact() })

    expect(host.querySelector('[data-testid="admin-recovery-error"]')?.textContent).toContain('Recovery could not be completed')
    expect(host.querySelector<HTMLInputElement>('[data-testid="admin-recovery-code"]')?.value).toBe('')
    expect(host.querySelector<HTMLInputElement>('[data-testid="admin-recovery-password"]')?.value).toBe('')
    expect(host.textContent).not.toContain('bad-code')
    expect(host.textContent).not.toContain('new-secret')
  })

  it('returns to fail-closed connection state when account-switch state cannot be refreshed', async () => {
    vi.mocked(getSnapshot).mockResolvedValueOnce(signedInSnapshot).mockRejectedValueOnce(new Error('snapshot refresh failed'))
    vi.mocked(authenticate).mockRejectedValueOnce(new Error('switch result uncertain'))
    await renderApp()
    await act(async () => { clickButton('Settings'); await settleReact() })
    await act(async () => { buttonByTestId('account-switch-toggle').click(); await settleReact() })
    setInputByTestId('account-switch-name', 'Rin')
    setInputByTestId('account-switch-password', 'uncertain-secret')
    await act(async () => { submitTestForm('account-switch-form'); await settleReact() })

    expect(host.textContent).toContain('Can’t reach Swarm right now')
    expect(host.querySelector('.workspace-panel')).toBeNull()
    expect(host.textContent).not.toContain('uncertain-secret')
  })

  it('returns to fail-closed connection state when recovery state cannot be refreshed', async () => {
    vi.mocked(getSnapshot).mockResolvedValueOnce(loginSnapshot).mockRejectedValueOnce(new Error('snapshot refresh failed'))
    vi.mocked(recoverAccount).mockRejectedValueOnce(new Error('recovery result uncertain'))
    await renderApp()
    await act(async () => { buttonByTestId('admin-recovery-toggle').click(); await settleReact() })
    setInputByTestId('admin-recovery-name', 'Admin Ada')
    setInputByTestId('admin-recovery-code', 'uncertain-recovery-code')
    setInputByTestId('admin-recovery-password', 'uncertain-new-password')
    await act(async () => { submitTestForm('admin-recovery-form'); await settleReact() })

    expect(host.textContent).toContain('Can’t reach Swarm right now')
    expect(host.querySelector('.auth-panel')).toBeNull()
    expect(host.querySelector('[data-testid="admin-recovery-form"]')).toBeNull()
    expect(host.textContent).not.toContain('uncertain-recovery-code')
    expect(host.textContent).not.toContain('uncertain-new-password')
  })

  it('clears an explicitly revealed Admin recovery code when Settings closes', async () => {
    vi.mocked(getSnapshot).mockResolvedValue(signedInSnapshot)
    await renderApp()
    await act(async () => { clickButton('Settings'); await settleReact() })
    await act(async () => { buttonByTestId('recovery-code-toggle').click(); await settleReact() })
    expect(host.querySelector('[data-testid="recovery-code-value"]')?.textContent).toContain('TEST-RECOVERY-CODE')
    await act(async () => { clickButton('Close settings'); await settleReact() })
    expect(host.querySelector('[role="dialog"]')).toBeNull()
    await act(async () => { clickButton('Settings'); await settleReact() })
    expect(host.querySelector('[data-testid="recovery-code-value"]')).toBeNull()
    expect(revealRecoveryCode).toHaveBeenCalledTimes(1)
  })

  it('localizes recovery and pending-discard controls in Japanese', async () => {
    await renderApp()
    await act(async () => { clickButton('日本語') })
    expect(host.querySelector('[data-testid="admin-recovery-toggle"]')?.textContent).toContain('管理者アカウントを復旧')
    await act(async () => { buttonByTestId('admin-recovery-toggle').click(); await settleReact() })
    expect(host.querySelector('[data-testid="admin-recovery-name"]')?.previousElementSibling?.textContent).toContain('管理者アカウント名')

    await act(async () => { root.unmount() })
    vi.mocked(getSnapshot).mockResolvedValue(pendingSnapshot)
    await renderApp()
    await act(async () => { clickButton('日本語') })
    expect(host.querySelector('[data-testid="pending-discard-toggle"]')?.textContent).toContain('保存済みの認証要求を破棄')
    await act(async () => { buttonByTestId('pending-discard-toggle').click(); await settleReact() })
    expect(host.querySelector('[data-testid="pending-discard-confirm"]')?.textContent).toContain('要求を破棄')
  })

  it('shows the desktop-only message in a browser without invoking the bridge', async () => {
    await renderApp(false)

    expect(host.textContent).toContain('Open Swarm desktop')
    expect(getSnapshot).not.toHaveBeenCalled()
    expect(authenticate).not.toHaveBeenCalled()
    expect(retryPending).not.toHaveBeenCalled()
  })
})
