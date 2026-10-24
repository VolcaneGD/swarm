import { afterEach, describe, expect, it, vi } from 'vitest'

afterEach(() => { vi.restoreAllMocks(); vi.resetModules() })

describe('localized dates', () => {
  it('preserves both existing locale formats and invalid input', async () => {
    const { localizedDate } = await import('./localizedDate')
    for (const language of ['ja', 'en'] as const) {
      for (const value of ['2026-10-22T00:00:00Z', '2026-01-01T23:59:59-08:00', '2026-03-08T10:01:00Z', '2000-02-29T12:34:56Z']) {
        expect(localizedDate(value, language)).toBe(new Intl.DateTimeFormat(
          language === 'ja' ? 'ja-JP' : 'en-US', { dateStyle: 'medium', timeStyle: 'short' },
        ).format(new Date(value)))
      }
      expect(localizedDate('not-a-date', language)).toBe('not-a-date')
    }
  })

  it('bounds native formatter allocations across1000 rows and repeated renders', async () => {
    vi.resetModules()
    const original = Intl.DateTimeFormat
    const constructors = vi.spyOn(Intl, 'DateTimeFormat').mockImplementation(function (locales, options) {
      return new original(locales, options)
    })
    const { localizedDate } = await import('./localizedDate')
    localizedDate('not-a-date', 'ja')
    expect(constructors).not.toHaveBeenCalled()
    for (let render = 0; render < 3; render++) {
      for (let row = 0; row < 1000; row++) {
        const value = new Date(Date.UTC(2026, 9, 22, 0, row)).toISOString()
        for (const language of ['ja', 'en'] as const) localizedDate(value, language)
      }
    }
    expect(constructors).toHaveBeenCalledTimes(2)
  })
})
