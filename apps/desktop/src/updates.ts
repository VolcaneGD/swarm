import { Channel, invoke } from '@tauri-apps/api/core'
import { useEffect, useState } from 'react'

export type AvailableUpdate = { version: string }
export type UpdateNotes = { version: string; notes: string }
let startupCheck: Promise<AvailableUpdate | null> | undefined
let startupNotes: Promise<UpdateNotes | null> | undefined

export function useStartupUpdate() {
  const [update, setUpdate] = useState<AvailableUpdate | null>(null)
  useEffect(() => {
    let active = true
    // One check per window lifetime, including StrictMode and account changes.
    startupCheck ??= invoke<AvailableUpdate | null>('gui_check_update').catch(() => null)
    void startupCheck.then(value => {
      if (active && value && typeof value.version === 'string') setUpdate(value)
    })
    return () => { active = false }
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

export function restartAfterUpdate(): Promise<void> {
  return invoke('gui_restart_after_update')
}
