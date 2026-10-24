import { createElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { describe, expect, it } from 'vitest'
import MarkdownBody from './MarkdownBody'

function render(body: string) {
  return renderToStaticMarkup(createElement(MarkdownBody, { children: body, language: 'ja', onOpenLink: () => {} }))
}
function reference(body: string) {
  return renderToStaticMarkup(createElement('div', { className: 'markdown-body', 'data-testid': 'markdown-body' },
    createElement(ReactMarkdown, { children: body, remarkPlugins: [remarkGfm], skipHtml: true })))
}

describe('literal paragraphs preserve Markdown output', () => {
  it('matches the real GFM renderer for short, long and Unicode literal paragraphs', () => {
    const samples = ['Hello world.', 'Sample Post 999, ready!', '日本語の本文です。', 'Cafe\u0301 100％', 'Mix 日本語 123.45%', 'Quotes "double" and \'single\'.', 'D'.repeat(100000)]
    const words = ['Alpha', '日本語', 'Cafe\u0301', 'Post 1', '漢字（注記）', 'Western']
    const suffixes = ['', '.', '、次です。', '  two spaces', '?!', '…']
    for (const first of words) for (const last of words) for (const suffix of suffixes) samples.push(`${first} ${last}${suffix}`)
    for (const sample of samples) expect(render(sample), sample.slice(0, 60)).toBe(reference(sample))
  })

  it('keeps nonliteral paragraph, list and formatting behavior on the Markdown path', () => {
    for (const body of ['', ' first', 'last ', '1. ordered', '2) ordered', '- item', '*italic*', '**bold**', '~~gone~~', '# Heading', 'line\nnext', 'a\n\nparagraph', 'escape \\*', '&amp;', 'a_b_c', '---']) {
      expect(render(body), body).toBe(reference(body))
    }
    expect(render('    indented')).toContain('markdown-code-disclosure')
  })

  it('preserves safe link interception for GFM autolinks and semantic GFM tables', () => {
    for (const text of ['www.example.com', 'WWW.EXAMPLE.COM', 'https://example.com', 'Alpha WWW.']) {
      expect(render(text)).toContain('data-testid="markdown-link"')
      expect(render(text)).not.toContain('<a ')
    }
    expect(render('person@example.com')).toContain('markdown-link-disabled')
    expect(render('| A | B |\n| - | - |\n| 1 | 2 |')).toContain('<table>')
    expect(render('![alt](https://images.invalid/a)')).toContain('markdown-image-placeholder')
  })
})
