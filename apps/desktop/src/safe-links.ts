const RAW_CONTROL_OR_BACKSLASH = /[\u0000-\u001f\u007f-\u009f\\\s]/u
const ENCODED_CONTROL_OR_BACKSLASH = /%(?:0[0-9a-f]|1[0-9a-f]|7f|5c)/i

/** Normalize a link for an explicit external-open confirmation. */
export function normalizeSafeExternalUrl(raw: string): string | null {
  if (typeof raw !== 'string' || RAW_CONTROL_OR_BACKSLASH.test(raw) || ENCODED_CONTROL_OR_BACKSLASH.test(raw)) return null
  const value = raw
  if (!value) return null
  const match = value.match(/^https?:\/\/([^/?#]*)/i)
  if (!match || !match[1] || match[1].includes('@')) return null
  try {
    const url = new URL(value)
    if ((url.protocol !== 'http:' && url.protocol !== 'https:') || !url.hostname || url.username || url.password) return null
    return url.href
  } catch {
    return null
  }
}
