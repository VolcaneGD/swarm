import { useCallback, useEffect, useRef, useState } from 'react'
import { guiDisplayDefaults, guiDisplaySettings, guiSetDisplaySettings } from './bridge'
import type { DisplaySettings } from './types'

const fallback: DisplaySettings = { language: 'en', theme: 'light' }
function checked(value: DisplaySettings): DisplaySettings {
  if (!value || !['en', 'ja'].includes(value.language) || !['light', 'dark'].includes(value.theme)) throw new Error('Invalid display settings')
  return value
}
export default function useDisplaySettings(accountId: string | null) {
  const [settings, setSettings] = useState(fallback)
  const [ready, setReady] = useState(false)
  const [saving, setSaving] = useState(false)
  const [failed, setFailed] = useState(false)
  const generation = useRef(0), mutation = useRef(0), running = useRef(false)
  const defaults = useRef(fallback)
  const refresh = useCallback(async () => {
    const request = ++generation.current
    setReady(false); setFailed(false)
    try {
      const saved = accountId ? (await guiDisplaySettings()).settings : null
      const value = checked(saved === null ? await guiDisplayDefaults() : saved)
      if (request !== generation.current) return
      if (saved === null) defaults.current = value
      setSettings(value); setReady(true)
    } catch { if (request === generation.current) setFailed(true) }
  }, [accountId])
  useEffect(() => {
    setSettings(defaults.current); running.current = false; setSaving(false)
    void refresh()
    return () => { generation.current++; mutation.current++ }
  }, [refresh])
  const change = async (patch: Partial<DisplaySettings>) => {
    if (!ready || running.current) return
    const next = checked({ ...settings, ...patch })
    if (!accountId) { setSettings(next); return }
    const request = ++mutation.current
    running.current = true; setSaving(true)
    try {
      const saved = checked(await guiSetDisplaySettings(accountId, next))
      if (request === mutation.current) { setSettings(saved); setFailed(false) }
    } catch { if (request === mutation.current) { setFailed(true); setReady(false) } }
    finally { if (request === mutation.current) { running.current = false; setSaving(false) } }
  }
  return { settings, ready, saving, failed, refresh, change }
}
