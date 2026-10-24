import { describe, expect, it } from 'vitest'
import { normalizeSafeExternalUrl } from './safe-links'

describe('normalizeSafeExternalUrl', () => {
  it.each([
    ['HTTPS://Example.COM:443/a/../guide?q=one%20two#section', 'https://example.com/guide?q=one%20two#section'],
    ['http://example.com:80/guide', 'http://example.com/guide'],
    ['https://[2001:db8::1]:443/posts/7', 'https://[2001:db8::1]/posts/7'],
    ['https://例え.テスト/こんにちは', 'https://xn--r8jz45g.xn--zckzah/%E3%81%93%E3%82%93%E3%81%AB%E3%81%A1%E3%81%AF'],
  ])('normalizes a safe web URL %s', (raw, normalized) => {
    expect(normalizeSafeExternalUrl(raw)).toBe(normalized)
  })

  it.each([
    'javascript:alert(1)',
    'data:text/html,<script>alert(1)</script>',
    'file:///C:/Users/secret.txt',
    'vbscript:msgbox(1)',
    '//example.com/path',
    'https://user:password@example.com/private',
    'http://name@example.com/',
    'https:\\\\evil.example/path',
    'https://example.com\\@evil.example/path',
    'https://example.com/a%5cb',
    'https://example.com/a%0d%0ab',
    'https://example.com/path\r\nX-Injected: yes',
    'https://example.com/\u0000path',
  ])('rejects unsafe URL %s', (raw) => {
    expect(normalizeSafeExternalUrl(raw)).toBeNull()
  })

  it('rejects non-string values at the runtime boundary', () => {
    expect(normalizeSafeExternalUrl(null as unknown as string)).toBeNull()
    expect(normalizeSafeExternalUrl(undefined as unknown as string)).toBeNull()
  })
})

