import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { isDesktopPreferenceHost } from './uiPreferenceStorage'

type FlushPreferences = () => Promise<void>
type CloseRequest = { request_id: number }

const flushers = new Set<FlushPreferences>()

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
      await flushRegisteredPreferences()
    } catch {
      success = false
    }
    await invoke('gui_close_ready', { requestId: payload.request_id, success })
  })
}
