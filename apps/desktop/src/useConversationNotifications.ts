import { useCallback, useEffect, useRef, useState } from 'react'
import { guiRead, guiSubscribeThread } from './bridge'
import type { ConversationUnread } from './types'

export default function useConversationNotifications(accountId: string, refreshToken: number) {
  const [subscribed, setSubscribed] = useState(new Set<string>())
  const [unread, setUnread] = useState<ConversationUnread>({ items: [], total: 0 })
  const [ready, setReady] = useState(false)
  const [failed, setFailed] = useState(false)
  const [saving, setSaving] = useState(false)
  const generation = useRef(0)
  const mutation = useRef(0)
  const running = useRef(false)
  const lastSnapshot = useRef<{ accountId: string; subscribed: Set<string>; unread: ConversationUnread } | null>(null)
  const refresh = useCallback(async () => {
    const request = ++generation.current
    try {
      const [subscriptions, counts] = await Promise.all([guiRead('conversation.subscriptions'), guiRead('conversation.unread')])
      if (request !== generation.current) return
      const nextSubscribed = new Set(subscriptions.items.map((item) => item.id))
      const previous = lastSnapshot.current
      if (previous?.accountId === accountId && previous.subscribed.size === nextSubscribed.size
        && [...previous.subscribed].every((id) => nextSubscribed.has(id))
        && previous.unread.total === counts.total && previous.unread.items.length === counts.items.length
        && previous.unread.items.every((item, index) => item.thread_id === counts.items[index].thread_id && item.unread_count === counts.items[index].unread_count)) return
      lastSnapshot.current = { accountId, subscribed: nextSubscribed, unread: counts }
      setSubscribed(nextSubscribed); setUnread(counts)
      setReady(true); setFailed(false)
    } catch { if (request === generation.current) { lastSnapshot.current = null; setReady(false); setFailed(true) } }
  }, [accountId])
  useEffect(() => { void refresh(); return () => { generation.current++ } }, [refresh, refreshToken])
  useEffect(() => {
    running.current = false; setSaving(false)
    return () => { mutation.current++ }
  }, [accountId])
  useEffect(() => {
    const timer = window.setInterval(() => { if (!running.current && document.visibilityState === 'visible') void refresh() }, 5000)
    return () => window.clearInterval(timer)
  }, [refresh])
  const toggle = async (id: string) => {
    if (!ready || running.current) return
    const request = ++mutation.current
    running.current = true; setSaving(true)
    try {
      await guiSubscribeThread(accountId, id, !subscribed.has(id))
      if (request === mutation.current) await refresh()
    } catch { if (request === mutation.current) { setReady(false); setFailed(true) } }
    finally { if (request === mutation.current) { running.current = false; setSaving(false) } }
  }
  return { subscribed, unread, ready, failed, saving, refresh, toggle }
}
