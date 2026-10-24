type Language = 'en' | 'ja'

// A formatter owns native ICU resources. Share the two display formats across
// rows and workspace refreshes instead of constructing one for every timestamp.
const formatters: Partial<Record<Language, Intl.DateTimeFormat>> = {}

export function localizedDate(value: string, language: Language): string {
  const date = new Date(value)
  if (Number.isNaN(date.valueOf())) return value
  const formatter = formatters[language] ??= new Intl.DateTimeFormat(
    language === 'ja' ? 'ja-JP' : 'en-US',
    { dateStyle: 'medium', timeStyle: 'short' },
  )
  return formatter.format(date)
}
