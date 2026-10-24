import { useEffect, useRef } from 'react'
import { guiRead } from './bridge'

type CapabilityOperation = 'management.capabilities' | 'authorization.capabilities' | 'rules.capabilities' | 'conversation.deleted'

// Ignore descriptive name changes; missing booleans, grants, scopes or deleted
// records revoke access. Deleted records are absent from live capabilities.
function reduced(previous: unknown, current: unknown): boolean {
  if (previous === true) return current !== true
  if (Array.isArray(previous)) {
    if (!Array.isArray(current)) return previous.length > 0
    return previous.some(old => {
      if (typeof old === 'string') return !current.includes(old)
      if (old && typeof old === 'object') {
        const key = 'id' in old ? 'id' : 'operation' in old ? 'operation' : null
        if (key) {
          const fresh = current.find(item => item && typeof item === 'object' && Reflect.get(item, key) === Reflect.get(old, key))
          return !fresh || reduced(old, fresh)
        }
      }
      return false
    })
  }
  if (previous && typeof previous === 'object') return Object.entries(previous).some(([key, value]) =>
    reduced(value, current && typeof current === 'object' ? Reflect.get(current, key) : undefined))
  return false
}

export default function useSettingsCapabilityGuard(accountId: string, operation: CapabilityOperation, expected: object | null, onAccessLost?: () => void) {
  const latest = useRef({ expected, onAccessLost })
  latest.current = { expected, onAccessLost }
  useEffect(() => {
    let active = true
    let inFlight = false
    let invalidated = false
    const check = async () => {
      const snapshot = latest.current.expected
      if (!active || invalidated || inFlight || !snapshot || !latest.current.onAccessLost || document.visibilityState !== 'visible') return
      inFlight = true
      const invalidate = () => {
        if (active && !invalidated && latest.current.expected === snapshot) {
          invalidated = true
          latest.current.onAccessLost?.()
        }
      }
      try {
        const current = await guiRead(operation)
        if (reduced(snapshot, current)) invalidate()
      } catch (error) {
        const exit = error && typeof error === 'object' && 'exit_code' in error ? error.exit_code : null
        if (exit === 3 || exit === 4 || exit === 5) invalidate()
      } finally { inFlight = false }
    }
    const refresh = () => { void check() }
    const timer = window.setInterval(refresh, 5000)
    window.addEventListener('focus', refresh)
    document.addEventListener('visibilitychange', refresh)
    return () => {
      active = false
      window.clearInterval(timer)
      window.removeEventListener('focus', refresh)
      document.removeEventListener('visibilitychange', refresh)
    }
  }, [accountId, operation])
}
