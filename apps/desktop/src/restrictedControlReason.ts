export function restrictedControlReason(target: EventTarget | null, language: 'ja' | 'en'): string | null {
  if (!(target instanceof Element)) return null
  const control = target.closest<HTMLElement>('button:disabled,input:disabled,select:disabled,textarea:disabled,[aria-disabled="true"]')
    || target.closest('label')?.querySelector<HTMLElement>('input:disabled,select:disabled,textarea:disabled')
  if (!control) return null
  const explicit = control.getAttribute('data-disabled-reason') || control.closest('[data-disabled-reason]')?.getAttribute('data-disabled-reason')
  if (explicit) return explicit
  const described = control.getAttribute('aria-describedby')?.split(/\s+/).map((id) => document.getElementById(id)?.textContent?.trim()).filter(Boolean).join(' ')
  if (described) return described
  if (control.closest('[data-restriction-state="busy"]')) return language === 'ja' ? '処理中です。完了してからもう一度操作してください。' : 'An operation is in progress. Try again after it finishes.'
  if (control.closest('[data-restriction-state="runtime"]')) return language === 'ja' ? 'バックグラウンドサービスを起動してから操作してください。' : 'Start the background service before continuing.'
  if (control.closest('.authorization-management')) return language === 'ja' ? '対象と範囲、現在の操作権限を確認してください。未保存の変更がない場合も保存できません。' : 'Check the subject, scope and current permissions. Saving also requires unsaved changes.'
  return language === 'ja' ? 'この操作は現在利用できません。必要な入力・選択、操作権限、処理状況を確認してください。' : 'This action is currently unavailable. Check required inputs, selections, permissions and operation status.'
}
