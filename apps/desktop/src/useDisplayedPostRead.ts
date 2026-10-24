import { useEffect, useRef, useState, type RefObject } from 'react'
import { guiMarkDisplayedPostsRead } from './bridge'

export default function useDisplayedPostRead(accountId: string, threadId: string | null, root: RefObject<HTMLElement | null>, scope: string, enabled: boolean, onMarked: () => void) {
  const [failed, setFailed] = useState(false)
  const [retry, setRetry] = useState(0)
  const onMarkedRef = useRef(onMarked)
  onMarkedRef.current = onMarked
  useEffect(() => {
    setFailed(false)
    const container = root.current
    if (!enabled || !threadId || !container || typeof IntersectionObserver === 'undefined') return
    let active = true, inFlight = false, blocked = false
    const visible = new Set<HTMLElement>()
    const acknowledged = new Set<string>()
    const flush = async () => {
      if (!active || blocked || inFlight || document.visibilityState !== 'visible' || !container.isConnected
        || container.closest('[inert], [aria-hidden="true"]') || document.querySelector('[role="dialog"][aria-modal="true"]')) return
      const boundary = container.getBoundingClientRect()
      const heading = container.querySelector('.thread-heading')?.getBoundingClientRect()
      const top = Math.max(0, boundary.top, heading?.bottom || 0)
      const bottom = Math.min(window.innerHeight, boundary.bottom)
      const ids = new Set<string>()
      for (const body of visible) {
        const post = body.closest<HTMLElement>('[data-post-id]')
        const id = post?.dataset.postId
        const rect = body.getBoundingClientRect()
        if (body.isConnected && container.contains(body) && id && !acknowledged.has(id) && rect.height > 0
          && rect.bottom > top && rect.top < bottom && rect.right > Math.max(0, boundary.left) && rect.left < Math.min(window.innerWidth, boundary.right)) ids.add(id)
        if (ids.size === 200) break
      }
      if (!ids.size || !active) return
      inFlight = true
      try {
        await guiMarkDisplayedPostsRead(accountId, threadId, [...ids])
        if (active) { ids.forEach((id) => acknowledged.add(id)); onMarkedRef.current() }
      } catch { if (active) { blocked = true; setFailed(true) } }
      finally { inFlight = false }
    }
    const observer = new IntersectionObserver((entries) => {
      if (!active) return
      for (const entry of entries) {
        if (entry.isIntersecting && entry.intersectionRatio > 0) visible.add(entry.target as HTMLElement)
        else visible.delete(entry.target as HTMLElement)
      }
    }, { root: container, threshold: [0, 0.01] })
    container.querySelectorAll<HTMLElement>('.timeline-post[data-post-id]:not(.is-deleted) > .markdown-body').forEach((body) => observer.observe(body))
    const timer = window.setInterval(() => void flush(), 300)
    return () => { active = false; observer.disconnect(); window.clearInterval(timer); visible.clear() }
  }, [accountId, threadId, root, scope, enabled, retry])
  return { failed, retry: () => setRetry((value) => value + 1) }
}
