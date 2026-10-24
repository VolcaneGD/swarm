import { isValidElement, memo, useMemo, type ReactNode } from 'react'
import ReactMarkdown, { type Components } from 'react-markdown'
import remarkGfm from 'remark-gfm'
import './MarkdownBody.css'
import { normalizeSafeExternalUrl } from './safe-links'

type Language = 'en' | 'ja'

// Only a single literal paragraph: no CommonMark delimiters, indentation,
// escapes, entities, email addresses or GFM URL prefixes. Everything else
// continues through the existing Markdown renderer.
function isLiteralParagraph(text: string): boolean {
  return /^[\p{L}\p{M}]/u.test(text)
    && text.trim() === text
    && /^[\p{L}\p{M}\p{N} "'.,!?、。！？「」『』（）…％%]+$/u.test(text)
    && !/www\./i.test(text)
}

function getCodeText(node: ReactNode): string {
  if (typeof node === 'string' || typeof node === 'number') return String(node)
  if (Array.isArray(node)) return node.map(getCodeText).join('')
  if (isValidElement<{ children?: ReactNode }>(node)) return getCodeText(node.props.children)
  return ''
}

function getCodeLanguage(node: ReactNode): string | null {
  if (!isValidElement<{ className?: string }>(node) || !node.props.className) return null
  const token = /(?:^|\s)language-([a-z0-9_+-]{1,32})(?:\s|$)/i.exec(node.props.className)?.[1]
  if (!token) return null
  const commonNames: Record<string, string> = {
    c: 'C',
    cpp: 'C++',
    css: 'CSS',
    html: 'HTML',
    js: 'JavaScript',
    json: 'JSON',
    md: 'Markdown',
    py: 'Python',
    sh: 'Shell',
    ts: 'TypeScript',
    tsx: 'TSX',
    yml: 'YAML',
  }
  const normalizedToken = token.toLowerCase()
  if (commonNames[normalizedToken]) return commonNames[normalizedToken]
  return token.replace(/[-_]+/g, ' ').replace(/\b\w/g, (letter) => letter.toUpperCase())
}

function MarkdownBody({
  children,
  language,
  onOpenLink,
}: {
  children: string | null | undefined
  language: Language
  onOpenLink: (normalizedUrl: string, opener: HTMLButtonElement) => void
}) {
  const imageText = language === 'ja' ? '画像を表示しません' : 'Image omitted'
  const components = useMemo<Components>(() => ({
    a({ href, children: linkChildren }) {
      const safeUrl = href ? normalizeSafeExternalUrl(href) : null
      if (!safeUrl) return <span className="markdown-link-disabled">{linkChildren}</span>
      return <button className="markdown-link" data-testid="markdown-link" type="button" onClick={(event) => onOpenLink(safeUrl, event.currentTarget)}>{linkChildren}</button>
    },
    img({ alt }) {
      const description = alt?.trim() || imageText
      return <span className="markdown-image-placeholder" data-testid="markdown-image-placeholder" role="img" aria-label={description}>{imageText}{alt?.trim() ? `: ${alt.trim()}` : ''}</span>
    },
    pre({ children: code }) {
      const codeText = getCodeText(code)
      const lineCount = Math.max(1, codeText.replace(/\n$/, '').split('\n').length)
      const codeLanguage = getCodeLanguage(code)
      const lineLabel = language === 'ja'
        ? `${lineCount}行`
        : `${lineCount} ${lineCount === 1 ? 'line' : 'lines'}`

      return <details className="markdown-code-disclosure" data-testid="markdown-code-disclosure" open>
        <summary className="markdown-code-summary">
          <span>{language === 'ja' ? 'コード' : 'Code'}</span>
          {codeLanguage && <span className="markdown-code-language">{codeLanguage}</span>}
          <span className="markdown-code-line-count">{lineLabel}</span>
        </summary>
        <pre className="markdown-code-block" data-testid="markdown-code-block">{code}</pre>
      </details>
    },
    table({ children: tableContent }) {
      return <div className="markdown-table-scroll"><table>{tableContent}</table></div>
    },
    input({ type, checked, disabled }) {
      if (type !== 'checkbox') return null
      return <input type="checkbox" checked={checked} disabled={disabled ?? true} readOnly aria-readonly="true" />
    },
  }), [imageText, language, onOpenLink])

  return <div className="markdown-body" data-testid="markdown-body">
    {children && isLiteralParagraph(children)
      ? <p>{children}</p>
      : <ReactMarkdown remarkPlugins={[remarkGfm]} skipHtml components={components}>{children || ''}</ReactMarkdown>}
  </div>
}

// Unread/subscription polling rerenders the workspace without changing bodies.
// Keep unchanged Markdown out of that parse path; every rendering input matters.
export default memo(MarkdownBody)
