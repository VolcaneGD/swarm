export const PASSWORD_FEEDBACK_EVENT = 'swarm-password-feedback'
export type PasswordFeedback = 'changed' | 'enabled' | 'disabled'
export function announcePasswordChange(kind: PasswordFeedback) {
  document.dispatchEvent(new CustomEvent(PASSWORD_FEEDBACK_EVENT, { detail: kind }))
}
