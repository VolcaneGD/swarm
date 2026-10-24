import { Channel, invoke } from '@tauri-apps/api/core'
import { useEffect, useState } from 'react'

export type AvailableUpdate = { version: string }
export type UpdateNotes = { version: string; notes: string }
let startupCheck: Promise<AvailableUpdate | null> | undefined
let startupNotes: Promise<UpdateNotes | null> | undefined
let latestUpdate: AvailableUpdate | null = null
let checkSequence = 0
const updateListeners = new Set<(update: AvailableUpdate | null) => void>()

export async function checkForUpdates(): Promise<AvailableUpdate | null> {
  const sequence = ++checkSequence
  const value = await invoke<AvailableUpdate | null>('gui_check_update')
  if (value !== null && (!value || typeof value.version !== 'string')) throw new Error('Invalid update response')
  if (sequence === checkSequence) {
    latestUpdate = value
    for (const listener of updateListeners) listener(value)
  }
  return value
}

export function useStartupUpdate() {
  const [update, setUpdate] = useState<AvailableUpdate | null>(latestUpdate)
  useEffect(() => {
    updateListeners.add(setUpdate)
    setUpdate(latestUpdate)
    // One automatic check per window; human-requested checks are independent.
    startupCheck ??= checkForUpdates().catch(() => null)
    return () => { updateListeners.delete(setUpdate) }
  }, [])
  return update
}

export type UpdateProgress = { phase: 'downloading' | 'verifying' | 'installing'; downloaded?: number; total?: number }
export function installUpdate(onProgress: (progress: UpdateProgress) => void): Promise<void> {
  const channel = new Channel<UpdateProgress>()
  channel.onmessage = onProgress
  return invoke('gui_install_update', { onProgress: channel })
}

export function useUpdateNotes() {
  const [notes, setNotes] = useState<UpdateNotes | null>(null)
  useEffect(() => {
    let active = true
    startupNotes ??= invoke<UpdateNotes | null>('gui_update_notes').catch(() => null)
    void startupNotes.then(value => {
      if (active && value && typeof value.version === 'string' && typeof value.notes === 'string') setNotes(value)
    })
    return () => { active = false }
  }, [])
  return notes
}
