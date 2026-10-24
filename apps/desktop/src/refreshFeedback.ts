export type RefreshFeedback = 'updating' | 'updated' | 'refreshFailed'
export const REFRESH_FEEDBACK_EVENT = 'swarm-refresh-feedback'

export function notifyRefreshFeedback(state: RefreshFeedback) {
  document.dispatchEvent(new CustomEvent<RefreshFeedback>(REFRESH_FEEDBACK_EVENT, { detail: state }))
}
