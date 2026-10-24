import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { isDesktopPreferenceHost } from './uiPreferenceStorage'

type FlushPreferences = () => Promise<void>
type CloseRequest = { request_id: number; reason?: 'update' }

const flushers = new Set<FlushPreferences>()
const updateBlockers = new Set<() => boolean>()

export function registerUpdateBlocker(blocked: () => boolean): () => void {
  updateBlockers.add(blocked)
  return () => { updateBlockers.delete(blocked) }
}

export function assertUpdateCanStart(): void {
  if ([...updateBlockers].some(blocked => blocked())) throw new Error('UPDATE_UNSAVED_WORK')
}

export function releaseUpdateInputLock(): void {
  if (typeof document !== 'undefined') document.body.inert = false
}

export function registerPreferenceFlush(flush: FlushPreferences): () => void {
  flushers.add(flush)
  return () => { flushers.delete(flush) }
}

export async function flushRegisteredPreferences(): Promise<void> {
  const results = await Promise.allSettled([...flushers].map((flush) => flush()))
  if (results.some((result) => result.status === 'rejected')) {
    throw new Error('One or more UI preference writes could not be confirmed.')
  }
}

export async function installNativeClosePreferenceListener(enabled = isDesktopPreferenceHost() && typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window): Promise<() => void> {
  if (!enabled) return () => undefined
  return listen<CloseRequest>('swarm-prepare-close', async ({ payload }) => {
    let success = true
    try {
      if (payload.reason === 'update') assertUpdateCanStart()
      await flushRegisteredPreferences()
      if (payload.reason === 'update') {
        assertUpdateCanStart()
        document.body.inert = true
      }
    } catch {
      success = false
    }
    try {
      const accepted = await invoke<boolean>('gui_close_ready', { requestId: payload.request_id, success })
      if (payload.reason === 'update' && (!success || !accepted)) releaseUpdateInputLock()
    } catch {
      if (payload.reason === 'update') releaseUpdateInputLock()
    }
  })
}
