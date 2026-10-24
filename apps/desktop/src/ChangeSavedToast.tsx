import { useCallback, useEffect, useRef, useState } from 'react'
import { getOperationStatuses, useOperationStatuses } from './operationStatus'
import { restrictedControlReason } from './restrictedControlReason'
import { REFRESH_FEEDBACK_EVENT, type RefreshFeedback } from './refreshFeedback'
import { PASSWORD_FEEDBACK_EVENT, type PasswordFeedback } from './changeFeedback'

type ToastMessage = { kind: 'saved' | 'failed' | 'guidance' | 'password' | RefreshFeedback; text?: string; password?: PasswordFeedback }

// Include explicit user changes; exclude reads, diagnostics, automatic read
// marking, receipt acknowledgements and pane/scroll preference persistence.
const changes = new Set([
  'gui_edit', 'gui_set_password', 'gui_google_link', 'gui_google_unlink',
  'account.create', 'account.disable', 'account.reset',
  'role.create', 'role.assign', 'permission.batch.save', 'rules.save', 'rules.recovery.apply',
  'display.settings.set', 'notification.settings.set', 'thread.pin', 'thread.subscribe',
  'retention.settings.set', 'backup.settings.set', 'project.register', 'gui_startup_set',
  'board.edit', 'thread.edit', 'board.delete', 'thread.delete', 'post.delete',
  'board.restore', 'thread.restore', 'post.restore',
])

export default function ChangeSavedToast({ language }: { language: 'ja' | 'en' }) {
  const statuses = useOperationStatuses()
  const seen = useRef(new Set(getOperationStatuses().filter((entry) => entry.state !== 'pending').map((entry) => entry.id)))
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const [message, setMessage] = useState<ToastMessage | null>(null)
  const show = useCallback((next: ToastMessage) => {
    setMessage(next)
    if (timer.current) clearTimeout(timer.current)
    timer.current = next.kind === 'updating' ? null : setTimeout(() => { setMessage(null); timer.current = null }, next.kind === 'saved' || next.kind === 'updated' ? 2000 : 4000)
  }, [])
  useEffect(() => {
    const onPassword = (event: Event) => {
      const kind = (event as CustomEvent<PasswordFeedback>).detail
      if (kind === 'changed' || kind === 'enabled' || kind === 'disabled') show({ kind: 'password', password: kind })
    }
    document.addEventListener(PASSWORD_FEEDBACK_EVENT, onPassword)
    return () => document.removeEventListener(PASSWORD_FEEDBACK_EVENT, onPassword)
  }, [show])
  useEffect(() => {
    const onRefresh = (event: Event) => {
      const kind = (event as CustomEvent<RefreshFeedback>).detail
      if (kind === 'updating' || kind === 'updated' || kind === 'refreshFailed') show({ kind })
    }
    document.addEventListener(REFRESH_FEEDBACK_EVENT, onRefresh)
    return () => document.removeEventListener(REFRESH_FEEDBACK_EVENT, onRefresh)
  }, [show])
  useEffect(() => {
    const currentIds = new Set(statuses.map((entry) => entry.id))
    seen.current = new Set([...seen.current].filter((id) => currentIds.has(id)))
    let outcome: 'saved' | 'failed' | null = null
    for (const entry of statuses) {
      if (entry.state === 'pending' || seen.current.has(entry.id)) continue
      seen.current.add(entry.id)
      if (changes.has(entry.operation) && !(entry.operation === 'gui_set_password' && entry.state === 'completed')) outcome = entry.state === 'completed' ? 'saved' : 'failed'
    }
    if (outcome) show({ kind: outcome })
  }, [statuses, show])
  useEffect(() => {
    const onRestrictedPress = (event: PointerEvent) => {
      if (event.button !== 0) return
      const reason = restrictedControlReason(event.target, language)
      if (!reason) return
      event.preventDefault(); event.stopPropagation()
      show({ kind: 'guidance', text: reason })
    }
    document.addEventListener('pointerdown', onRestrictedPress, true)
    const onRestrictedKey = (event: KeyboardEvent) => {
      if (event.key !== 'Enter' && event.key !== ' ') return
      const reason = restrictedControlReason(event.target, language)
      if (!reason) return
      event.preventDefault(); event.stopPropagation()
      show({ kind: 'guidance', text: reason })
    }
    document.addEventListener('keydown', onRestrictedKey, true)
    return () => { document.removeEventListener('pointerdown', onRestrictedPress, true); document.removeEventListener('keydown', onRestrictedKey, true) }
  }, [language, show])
  useEffect(() => () => { if (timer.current) clearTimeout(timer.current) }, [])
  const isError = message?.kind === 'failed' || message?.kind === 'refreshFailed'
  const guidance = isError || message?.kind === 'guidance'
  const text = message?.kind === 'updating' ? language === 'ja' ? '更新中' : 'Updating'
    : message?.kind === 'password' ? message.password === 'disabled' ? language === 'ja' ? 'パスワードロックを無効にしました' : 'Password lock disabled'
      : message.password === 'enabled' ? language === 'ja' ? 'パスワードロックを有効にしました' : 'Password lock enabled'
      : language === 'ja' ? 'パスワードを変更しました' : 'Password changed'
    : message?.kind === 'updated' ? language === 'ja' ? '更新完了' : 'Update complete'
    : message?.kind === 'refreshFailed' ? language === 'ja' ? '更新に失敗しました' : 'Could not update'
    : message?.kind === 'guidance' ? message.text
    : message?.kind === 'failed' ? language === 'ja' ? '保存に失敗しました' : 'Could not save changes'
    : language === 'ja' ? '変更を保存しました' : 'Changes saved'
  return message ? <div className={`change-saved-toast${guidance ? ' guidance-toast' : ''}`} data-testid={guidance ? 'guidance-toast' : 'change-saved-toast'} role={isError ? 'alert' : 'status'} aria-live={isError ? 'assertive' : 'polite'}>{text}</div> : null
}
