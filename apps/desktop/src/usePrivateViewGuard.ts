import { useEffect, useRef } from 'react'
import { guiRead } from './bridge'
import type { ConversationCapabilities } from './types'

function lostRights(previous: ConversationCapabilities, current: ConversationCapabilities) {
  const reduced = (old: object, fresh: object) => Object.entries(old).some(([key, value]) => value === true && Reflect.get(fresh, key) !== true)
  return reduced(previous, current) || (['boards', 'threads'] as const).some(kind => previous[kind].some(old => {
    const fresh = current[kind].find(item => item.id === old.id)
    return !fresh || reduced(old, fresh)
  }))
}

// Revalidate cached views without fetching another Post body. All valid Post
// numbers start at 1; this empty range still runs the Service permission checks.
export default function usePrivateViewGuard(accountId: string, threadId: string | null, onAccessLost: () => void, onSessionExpired?: () => void, capabilities?: ConversationCapabilities | null) {
  const callbacks = useRef({ onAccessLost, onSessionExpired, capabilities })
  callbacks.current = { onAccessLost, onSessionExpired, capabilities }
  useEffect(() => {
    let active = true
    let inFlight = false
    const check = async () => {
      if (!active || inFlight || document.visibilityState !== 'visible') return
      inFlight = true
      const expected = callbacks.current.capabilities
      try {
        if (threadId) await guiRead('post.list', { thread_id: threadId, before_no: 1, limit: 1 })
        if (!threadId || expected) {
          const current = await guiRead('conversation.capabilities')
          if (active && expected && callbacks.current.capabilities === expected && lostRights(expected, current)) callbacks.current.onAccessLost()
        }
      } catch (error) {
        if (!active || typeof error !== 'object' || error === null || !('exit_code' in error)) return
        const exit = (error as { exit_code?: unknown }).exit_code
        if (exit === 3 || exit === 4 || exit === 5) {
          callbacks.current.onAccessLost()
          if (exit === 3) callbacks.current.onSessionExpired?.()
        }
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
  }, [accountId, threadId])
}
