// @vitest-environment jsdom
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import MarkdownBody from './MarkdownBody'

let host: HTMLDivElement
let root: Root

async function render(children: string, onOpenLink = vi.fn(), language: 'en' | 'ja' = 'en') {
  host = document.createElement('div')
  document.body.append(host)
  root = createRoot(host)
  await act(async () => {
    root.render(<MarkdownBody children={children} language={language} onOpenLink={onOpenLink} />)
    await new Promise<void>((resolve) => setTimeout(resolve, 0))
  })
  return onOpenLink
}

beforeEach(() => {
  Object.defineProperty(globalThis, 'IS_REACT_ACT_ENVIRONMENT', { value: true, configurable: true })
})

afterEach(async () => {
  if (root) await act(async () => root.unmount())
  host?.remove()
})

describe('MarkdownBody', () => {
  it('renders headings, lists, quotes, GFM tables, and fenced code as semantic elements', async () => {
    await render([
      '# Release notes',
      '',
      '- first item',
      '- second item',
      '',
      '> reviewed quote',
      '',
      '| Area | State |',
      '| --- | --- |',
      '| Search | Ready |',
      '',
      '```ts',
      'const answer = 42',
      '```',
    ].join('\n'))

    expect(host.querySelector('[data-testid="markdown-body"] h1')?.textContent).toBe('Release notes')
    expect(host.querySelectorAll('ul > li')).toHaveLength(2)
    expect(host.querySelector('blockquote')?.textContent).toContain('reviewed quote')
    expect(host.querySelector('.markdown-table-scroll table th')?.textContent).toBe('Area')
    expect(host.querySelector('.markdown-table-scroll table td')?.textContent).toBe('Search')
    expect(host.querySelector('[data-testid="markdown-code-block"] code')?.textContent).toBe('const answer = 42\n')
  })

  it('keeps raw HTML inert and represents Markdown images without creating an image request', async () => {
    const markup = [
      '<script>window.__markdownExecuted = true</script>',
      '<img src="https://tracking.invalid/pixel.png" onerror="window.__markdownExecuted = true">',
      '',
      '![diagram](https://images.invalid/private.png "title")',
    ].join('\n')
    await render(markup)

    expect(host.querySelector('script')).toBeNull()
    expect(host.querySelector('img')).toBeNull()
    expect(host.querySelectorAll('[data-testid="markdown-image-placeholder"]')).toHaveLength(1)
    expect(host.querySelector('[data-testid="markdown-image-placeholder"]')?.textContent).toContain('diagram')
    expect((window as Window & { __markdownExecuted?: boolean }).__markdownExecuted).toBeUndefined()
  })

  it('wraps fenced and indented code blocks in independent open disclosures without changing their text', async () => {
    await render([
      '```ts',
      'const first = 1',
      'const second = 2',
      '```',
      '',
      '    <img src="https://unsafe.invalid/image.png">',
    ].join('\n'))

    const disclosures = host.querySelectorAll<HTMLDetailsElement>('[data-testid="markdown-code-disclosure"]')
    expect(disclosures).toHaveLength(2)
    expect([...disclosures].every((disclosure) => disclosure.open)).toBe(true)
    expect(disclosures[0].querySelector('summary')?.tagName).toBe('SUMMARY')
    expect(disclosures[0].querySelector('summary')?.textContent).toContain('TypeScript')
    expect(disclosures[0].querySelector('summary')?.textContent).toContain('2 lines')
    expect(disclosures[0].querySelector('code')?.textContent).toBe('const first = 1\nconst second = 2\n')
    expect(disclosures[1].querySelector('code')?.textContent).toBe('<img src="https://unsafe.invalid/image.png">\n')
    expect(host.querySelector('img')).toBeNull()
  })

  it('localizes the code disclosure label and lets each native disclosure toggle independently', async () => {
    await render(['```js', 'const one = 1', '```', '', '```json', '{ "two": 2 }', '```'].join('\n'), vi.fn(), 'ja')

    const disclosures = host.querySelectorAll<HTMLDetailsElement>('[data-testid="markdown-code-disclosure"]')
    expect(disclosures).toHaveLength(2)
    expect(disclosures[0].querySelector('summary')?.textContent).toContain('コード')
    expect(disclosures[0].querySelector('summary')?.textContent).not.toContain('Code')
    expect(disclosures[0].querySelector('summary')?.textContent).toContain('1行')

    await act(async () => { disclosures[0].querySelector('summary')?.dispatchEvent(new MouseEvent('click', { bubbles: true })) })
    expect(disclosures[0].open).toBe(false)
    expect(disclosures[1].open).toBe(true)
  })

  it('renders invalid links as inert text and sends only a normalized safe URL to the open callback', async () => {
    const callback = vi.fn()
    await render('[safe](HTTPS://Example.COM:443/a/../guide?q=one%20two) and [unsafe](javascript:alert%281%29)', callback)
    const links = host.querySelectorAll<HTMLButtonElement>('[data-testid="markdown-link"]')
    expect(links).toHaveLength(1)
    expect(host.textContent).toContain('unsafe')
    expect(host.querySelector('a')).toBeNull()

    await act(async () => { links[0].click() })
    expect(callback).toHaveBeenCalledWith('https://example.com/guide?q=one%20two', links[0])
    expect(callback).toHaveBeenCalledTimes(1)
  })
})

