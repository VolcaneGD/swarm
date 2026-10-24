// Keep these limits aligned with PROJECT_SPEC.md and the Rust operation layer.
export const BOARD_NAME_LIMIT = 100
export const THREAD_NAME_LIMIT = 200
export const BODY_LIMIT = 100_000
export const BODY_BYTE_LIMIT = 1024 * 1024

// Unicode scalar values, rather than UTF-16 code units, match Rust str::chars.
export function characterCount(text: string): number {
  return Array.from(text).length
}

export function characterCounter(text: string, limit: number, language: 'en' | 'ja'): string {
  return `${characterCount(text).toLocaleString(language)} / ${limit.toLocaleString(language)} ${language === 'ja' ? '文字' : 'characters'}`
}
