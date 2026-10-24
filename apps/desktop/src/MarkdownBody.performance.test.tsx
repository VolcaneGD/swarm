// @vitest-environment jsdom
import { act } from 'react'
import { createRoot } from 'react-dom/client'
import { expect, it, vi } from 'vitest'

const parse = vi.hoisted(() => vi.fn())
vi.mock('react-markdown', async (importOriginal) => {
  const module = await importOriginal<typeof import('react-markdown')>()
  return { ...module, default: (props: Parameters<typeof module.default>[0]) => {
    parse()
    return module.default(props)
  } }
})
import MarkdownBody from './MarkdownBody'

it('skips parsing on unrelated parent updates but reflects all Markdown inputs', async () => {
  Object.defineProperty(globalThis, 'IS_REACT_ACT_ENVIRONMENT', { value: true, configurable: true })
  const host = document.createElement('div')
  document.body.append(host)
  const root = createRoot(host)
  const first = vi.fn(), second = vi.fn()
  const update = async (tick: number, body: string, language: 'ja' | 'en', callback = first) => {
    await act(async () => root.render(<section data-tick={tick}>
      <MarkdownBody language={language} onOpenLink={callback}>{body}</MarkdownBody>
    </section>))
  }
  try {
    parse.mockClear()
    const body = '[Example](https://example.com/)\n\n![](https://images.invalid/example.png)'
    await update(0, body, 'en')
    for (let tick = 1; tick <= 10; tick++) await update(tick, body, 'en')
    expect(parse).toHaveBeenCalledTimes(1)
    await update(11, `${body}\n\n**Changed**`, 'en')
    expect(parse).toHaveBeenCalledTimes(2)
    expect(host.querySelector('strong')?.textContent).toBe('Changed')
    await update(12, `${body}\n\n**Changed**`, 'ja')
    expect(parse).toHaveBeenCalledTimes(3)
    expect(host.querySelector('[role="img"]')?.textContent).toBe('画像を表示しません')
    await update(13, `${body}\n\n**Changed**`, 'ja', second)
    await act(async () => host.querySelector<HTMLButtonElement>('.markdown-link')!.click())
    expect(parse).toHaveBeenCalledTimes(4)
    expect(first).not.toHaveBeenCalled()
    expect(second).toHaveBeenCalledWith('https://example.com/', expect.any(HTMLButtonElement))
  } finally {
    await act(async () => root.unmount())
    host.remove()
  }
})
