export default function UnreadBadge({ count, language }: { count: number; language: 'en' | 'ja' }) {
  return count > 0 ? <span className="unread-badge" aria-label={language === 'ja' ? `未読${count}件` : `${count} unread posts`}>{count >= 99 ? '99+' : count}</span> : null
}
