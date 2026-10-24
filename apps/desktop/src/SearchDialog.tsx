import { useEffect, useRef, useState, type FormEvent, type RefObject } from 'react'
import { guiRead } from './bridge'
import Tabs from './Tabs'
import { localizedDate as displayDate } from './localizedDate'
import type { Board, SearchArgs, SearchItem, SearchPage, Thread } from './types'

type Language = 'en' | 'ja'
type SearchRange = 'thread' | 'board' | 'all'
type Copy = typeof words.en | typeof words.ja

const words = {
  en: {
    title: 'Search', close: 'Close search', query: 'Search terms', range: 'Search location',
    threadRange: 'This Thread', boardRange: 'All Threads in this Board', allRange: 'All Threads across all Boards',
    chooseThread: 'Choose a Thread', chooseBoard: 'Choose a Board', projectId: 'Project ID', authorId: 'Author ID',
    from: 'From date', to: 'Through date', contentFilters: 'Project and author filters apply to Posts and Responses.',
    dateOrder: 'The start date must be on or before the end date.',
    submit: 'Search', searching: 'Searching…', retry: 'Retry search', more: 'Load more results', loadingMore: 'Loading more…', retryMore: 'Retry loading more',
    empty: 'No matching results.', error: 'Could not search. Check access and retry.', moreError: 'Could not load more results. Retry to continue.',
    queryRequired: 'Enter search terms.', chooseThreadError: 'Choose a readable Thread.', chooseBoardError: 'Choose a readable Board.',
    board: 'Board', thread: 'Thread', post: 'Post', response: 'Response', no: 'No.',
    navigationError: 'This result is no longer available or cannot be opened. Refresh the workspace and try again.',
  },
  ja: {
    title: '検索', close: '検索を閉じる', query: '検索語', range: '検索範囲',
    threadRange: 'このスレッド', boardRange: 'このボードの全スレッド', allRange: 'すべてのボードの全スレッド',
    chooseThread: 'スレッドを選択', chooseBoard: 'ボードを選択', projectId: 'Project ID', authorId: '投稿者ID',
    from: '開始日', to: '終了日', contentFilters: 'Project IDと投稿者IDの絞り込みはPost・Responseに適用されます。',
    dateOrder: '開始日は終了日以前の日付にしてください。',
    submit: '検索', searching: '検索中…', retry: '検索を再試行', more: '検索結果をさらに表示', loadingMore: '読み込み中…', retryMore: '続きを再試行',
    empty: '一致する検索結果はありません。', error: '検索できませんでした。閲覧権限を確認して再試行してください。', moreError: '続きの結果を読み込めませんでした。再試行してください。',
    queryRequired: '検索語を入力してください。', chooseThreadError: '閲覧できるスレッドを選択してください。', chooseBoardError: '閲覧できるボードを選択してください。',
    board: 'Board', thread: 'Thread', post: 'Post', response: 'Response', no: 'No.',
    navigationError: 'この検索結果は利用できないか、開けません。ワークスペースを更新して再試行してください。',
  },
} as const

function dateBoundary(value: string, end: boolean): string | undefined {
  if (!value) return undefined
  const [year, month, day] = value.split('-').map(Number)
  const date = new Date(year, month - 1, day, 0, 0, 0, 0)
  if (Number.isNaN(date.valueOf())) return undefined
  if (end) date.setHours(23, 59, 59, 999)
  return date.toISOString()
}

function resultKey(item: SearchItem) {
  return `${item.kind}|${item.id}`
}

function uniqueResults(items: SearchItem[]) {
  const seen = new Set<string>()
  return items.filter((item) => {
    const key = resultKey(item)
    if (seen.has(key)) return false
    seen.add(key)
    return true
  })
}

function resultTitle(item: SearchItem, labels: Copy) {
  if (item.kind === 'board' || item.kind === 'thread') return item.name || item.id
  return `${item.kind === 'response' ? labels.response : labels.post} · ${labels.no} ${item.post_no ?? '—'}`
}

export default function SearchDialog({
  language,
  accountId,
  boards,
  threads,
  currentThread,
  busy,
  creationPending,
  refreshToken,
  openerRef,
  onClose,
  onNavigate,
  hasHighlight,
  onClearHighlight,
  initialQuery = '',
  onQueryChange,
}: {
  language: Language
  accountId: string
  boards: Board[]
  threads: Thread[]
  currentThread: Thread | null
  busy: boolean
  creationPending: boolean
  refreshToken: number
  openerRef: RefObject<HTMLButtonElement | null>
  onClose: () => void
  onNavigate: (item: SearchItem) => Promise<void>
  hasHighlight: boolean
  onClearHighlight: () => void
  initialQuery?: string
  onQueryChange?: (query: string) => void
}) {
  const t = words[language]
  const [target, setTarget] = useState<'body' | 'board_name' | 'thread_name'>('body')
  const [query, setQuery] = useState(initialQuery)
  const [range, setRange] = useState<SearchRange>('all')
  const [threadId, setThreadId] = useState(currentThread?.id || '')
  const initialBoard = currentThread?.board_id && boards.some((board) => board.id === currentThread.board_id)
    ? currentThread.board_id
    : boards[0]?.id || ''
  const [boardId, setBoardId] = useState(initialBoard)
  const [projectId, setProjectId] = useState('')
  const [authorId, setAuthorId] = useState('')
  const [fromDate, setFromDate] = useState('')
  const [toDate, setToDate] = useState('')
  const [results, setResults] = useState<SearchItem[]>([])
  const [nextCursor, setNextCursor] = useState<string | null>(null)
  const [totalCount, setTotalCount] = useState<number | null>(null)
  const [submittedArgs, setSubmittedArgs] = useState<SearchArgs | null>(null)
  const [loading, setLoading] = useState(false)
  const [loadingMore, setLoadingMore] = useState(false)
  const [searchError, setSearchError] = useState(false)
  const [moreError, setMoreError] = useState(false)
  const [validationError, setValidationError] = useState('')
  const requestGeneration = useRef(0)
  const mounted = useRef(false)
  const contextRef = useRef({ accountId, refreshToken })
  const searchInput = useRef<HTMLInputElement | null>(null)
  const dialogElement = useRef<HTMLElement | null>(null)
  const moreRunning = useRef(false)
  const lastCursor = useRef<string | null>(null)
  const blocked = busy || creationPending

  function clearSearch() {
    ++requestGeneration.current
    moreRunning.current = false
    lastCursor.current = null
    setQuery('')
    onQueryChange?.('')
    setTotalCount(null)
    setProjectId('')
    setAuthorId('')
    setFromDate('')
    setToDate('')
    setResults([])
    setNextCursor(null)
    setSubmittedArgs(null)
    setLoading(false)
    setLoadingMore(false)
    setSearchError(false)
    setMoreError(false)
    setValidationError('')
    onClearHighlight()
    searchInput.current?.focus()
  }

  useEffect(() => {
    mounted.current = true
    const opener = openerRef.current
    const rootOverflow = document.documentElement.style.overflow
    const bodyOverflow = document.body.style.overflow
    document.documentElement.style.overflow = 'hidden'
    document.body.style.overflow = 'hidden'
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault()
        onClose()
        return
      }
      if (event.key !== 'Tab') return
      const focusable = [...(dialogElement.current?.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), select:not(:disabled), [tabindex]:not([tabindex="-1"])') || [])].filter((element) => !element.closest('[hidden]') && element.tabIndex >= 0)
      if (!focusable?.length) { event.preventDefault(); return }
      const first = focusable[0]
      const last = focusable[focusable.length - 1]
      if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus() }
      else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus() }
    }
    searchInput.current?.focus()
    document.addEventListener('keydown', onKeyDown)
    return () => {
      mounted.current = false
      ++requestGeneration.current
      document.removeEventListener('keydown', onKeyDown)
      document.documentElement.style.overflow = rootOverflow
      document.body.style.overflow = bodyOverflow
      if (opener?.isConnected && !opener.disabled) opener.focus()
    }
  }, [onClose, openerRef])

  useEffect(() => {
    if (contextRef.current.accountId === accountId && contextRef.current.refreshToken === refreshToken) return
    contextRef.current = { accountId, refreshToken }
    setResults([])
    setNextCursor(null)
    setSubmittedArgs(null)
    setSearchError(false)
    setMoreError(false)
    setValidationError('')
    ++requestGeneration.current
    onClose()
  }, [accountId, onClose, refreshToken])

  function makeArgs(): SearchArgs | null {
    const normalizedQuery = query.trim()
    if (!normalizedQuery) {
      setValidationError(t.queryRequired)
      return null
    }
    if (target !== 'body') {
      setValidationError('')
      return { query: normalizedQuery, scope: target, limit: 50 }
    }
    if (range === 'thread' && !threads.some((thread) => thread.id === threadId)) {
      setValidationError(t.chooseThreadError)
      return null
    }
    if (range === 'board' && !boards.some((board) => board.id === boardId)) {
      setValidationError(t.chooseBoardError)
      return null
    }
    if (fromDate && toDate && fromDate > toDate) {
      setValidationError(t.dateOrder)
      return null
    }
    setValidationError('')
    return {
      query: normalizedQuery,
      scope: 'body',
      ...(range === 'thread' ? { thread_id: threadId } : {}),
      ...(range === 'board' ? { board_id: boardId } : {}),
      ...(projectId.trim() ? { project_id: projectId.trim() } : {}),
      ...(authorId.trim() ? { author_id: authorId.trim() } : {}),
      ...(dateBoundary(fromDate, false) ? { from: dateBoundary(fromDate, false) } : {}),
      ...(dateBoundary(toDate, true) ? { to: dateBoundary(toDate, true) } : {}),
      limit: 50,
    }
  }

  async function submitSearch(event?: FormEvent<HTMLFormElement>, retryArgs?: SearchArgs) {
    event?.preventDefault()
    if (blocked || loading || loadingMore) return
    const args = retryArgs || makeArgs()
    if (!args) return
    const request = ++requestGeneration.current
    setSubmittedArgs(args)
    setTotalCount(null)
    setResults([])
    setNextCursor(null)
    lastCursor.current = null
    setSearchError(false)
    setMoreError(false)
    setLoading(true)
    try {
      const page: SearchPage = await guiRead('search', args)
      if (!mounted.current || request !== requestGeneration.current) return
      setResults(uniqueResults(page.items))
      setTotalCount(page.total_count)
      setNextCursor(page.next_cursor)
    } catch {
      if (!mounted.current || request !== requestGeneration.current) return
      setSearchError(true)
    } finally {
      if (mounted.current && request === requestGeneration.current) setLoading(false)
    }
  }

  async function loadMore() {
    if (blocked || loading || moreRunning.current || !submittedArgs || !nextCursor) return
    if (lastCursor.current === nextCursor) { setMoreError(true); setNextCursor(null); return }
    const cursor = nextCursor
    const request = requestGeneration.current
    moreRunning.current = true
    setLoadingMore(true)
    setMoreError(false)
    try {
      const page: SearchPage = await guiRead('search', { ...submittedArgs, cursor })
      if (!mounted.current || request !== requestGeneration.current) return
      lastCursor.current = cursor
      setTotalCount(page.total_count)
      setResults((current) => {
        const seen = new Set(current.map(resultKey))
        const merged = [...current]
        for (const item of page.items) {
          const key = resultKey(item)
          if (seen.has(key)) continue
          seen.add(key)
          merged.push(item)
        }
        return merged
      })
      if (page.next_cursor === cursor) {
        setNextCursor(null)
        setMoreError(true)
      } else setNextCursor(page.next_cursor)
    } catch {
      if (!mounted.current || request !== requestGeneration.current) return
      setMoreError(true)
    } finally {
      if (mounted.current && request === requestGeneration.current) {
        moreRunning.current = false
        setLoadingMore(false)
      }
    }
  }

  function selectResult(item: SearchItem) {
    if (blocked) return
    onClose()
    void onNavigate(item)
  }

  function invalidateResults(clearHighlight = true) {
    if (clearHighlight) onClearHighlight()
    setTotalCount(null)
    ++requestGeneration.current
    moreRunning.current = false
    lastCursor.current = null
    setResults([])
    setNextCursor(null)
    setSubmittedArgs(null)
    setLoading(false)
    setLoadingMore(false)
    setSearchError(false)
    setMoreError(false)
  }

  const boardNames = new Map(boards.map((board) => [board.id, board.name]))
  const canSubmit = !blocked && !loading && !loadingMore

  return <div className="search-backdrop">
    <section className="search-dialog" data-testid="search-dialog" role="dialog" aria-modal="true" aria-label={t.title} ref={dialogElement}>
      <Tabs id="search" label={language === 'ja' ? '検索対象' : 'Search target'} value={target} disabled={blocked}
        items={[
          { value: 'body', label: language === 'ja' ? '本文検索' : 'Body search' },
          { value: 'board_name', label: language === 'ja' ? 'ボード名検索' : 'Board name search' },
          { value: 'thread_name', label: language === 'ja' ? 'スレッド名検索' : 'Thread name search' },
        ]} onChange={(next) => { invalidateResults(false); setValidationError(''); setTarget(next) }} />
      {(['body', 'board_name', 'thread_name'] as const).filter((value) => value !== target).map((value) => <div key={value} role="tabpanel" id={`search-panel-${value}`} aria-labelledby={`search-tab-${value}`} hidden />)}
      <div role="tabpanel" id={`search-panel-${target}`} aria-labelledby={`search-tab-${target}`} className="search-target-panel">
      <form className="search-form" onSubmit={(event) => void submitSearch(event)}>
        <label className="field"><span>{t.query}</span><input ref={searchInput} data-testid="search-query" value={query} onChange={(event) => { invalidateResults(); setQuery(event.target.value); onQueryChange?.(event.target.value); setValidationError('') }} disabled={blocked || loading || loadingMore} /></label>
        {target === 'body' && <fieldset className="search-range-group" disabled={blocked || loading || loadingMore}>
          <legend>{t.range}</legend>
          <label><input data-testid="search-range-thread" type="radio" name="search-range" value="thread" checked={range === 'thread'} onChange={() => { invalidateResults(); setRange('thread') }} /><span>{t.threadRange}</span></label>
          <label><input data-testid="search-range-board" type="radio" name="search-range" value="board" checked={range === 'board'} onChange={() => { invalidateResults(); setRange('board') }} /><span>{t.boardRange}</span></label>
          <label><input data-testid="search-range-all" type="radio" name="search-range" value="all" checked={range === 'all'} onChange={() => { invalidateResults(); setRange('all') }} /><span>{t.allRange}</span></label>
        </fieldset>}
        {target === 'body' && range === 'thread' && <label className="field"><span>{t.thread}</span><select data-testid="search-thread" value={threadId} onChange={(event) => { invalidateResults(); setThreadId(event.target.value) }} disabled={blocked || loading || loadingMore}>
          <option value="">{t.chooseThread}</option>{threads.map((thread) => <option value={thread.id} key={thread.id}>{thread.name}</option>)}
        </select></label>}
        {target === 'body' && range === 'board' && <label className="field"><span>{t.board}</span><select data-testid="search-board" value={boardId} onChange={(event) => { invalidateResults(); setBoardId(event.target.value) }} disabled={blocked || loading || loadingMore}>
          <option value="">{t.chooseBoard}</option>{boards.map((board) => <option value={board.id} key={board.id}>{board.name}</option>)}
        </select></label>}
        {target === 'body' && <details className="search-optional-filters" data-testid="search-optional-filters">
          <summary data-testid="search-optional-filters-summary">{language === 'ja' ? '絞り込み' : 'Filters'}</summary>
          <div className="search-optional-filter-content">
            <div className="search-filter-grid">
              <label className="field"><span>{t.projectId}</span><input data-testid="search-project-id" value={projectId} onChange={(event) => { invalidateResults(); setProjectId(event.target.value) }} disabled={blocked || loading || loadingMore} /></label>
              <label className="field"><span>{t.authorId}</span><input data-testid="search-author-id" value={authorId} onChange={(event) => { invalidateResults(); setAuthorId(event.target.value) }} disabled={blocked || loading || loadingMore} /></label>
              <label className="field"><span>{t.from}</span><input data-testid="search-from" type="date" value={fromDate} onChange={(event) => { invalidateResults(); setFromDate(event.target.value); setValidationError('') }} disabled={blocked || loading || loadingMore} /></label>
              <label className="field"><span>{t.to}</span><input data-testid="search-to" type="date" value={toDate} onChange={(event) => { invalidateResults(); setToDate(event.target.value); setValidationError('') }} disabled={blocked || loading || loadingMore} /></label>
            </div>
            <p className="search-filter-hint">{t.contentFilters}</p>
          </div>
        </details>}
        {validationError && <p className="search-error" data-testid="search-validation-error" role="alert">{validationError}</p>}
        <div className="search-actions"><button className="button button-primary" data-testid="search-submit" type="submit" disabled={!canSubmit}>{loading ? t.searching : t.submit}</button><button className="button button-secondary" data-testid="search-clear" type="button" onClick={clearSearch} disabled={blocked || !(hasHighlight || query || projectId || authorId || fromDate || toDate || submittedArgs || validationError)}>{language === 'ja' ? '解除' : 'Clear'}</button></div>
      </form>
      {loading && <p className="search-status" data-testid="search-loading" role="status">{t.searching}</p>}
      {searchError && <div className="search-error-block"><p className="search-error" data-testid="search-error" role="alert">{t.error}</p><button className="button button-secondary" data-testid="search-retry" type="button" onClick={() => void submitSearch(undefined, submittedArgs || undefined)} disabled={blocked || loading}>{t.retry}</button></div>}
      {!loading && !searchError && submittedArgs && totalCount !== null && <p className="search-status" data-testid="search-result-count" role="status">{language === 'ja' ? `検索結果：${totalCount.toLocaleString('ja-JP')}件` : `Search results: ${totalCount.toLocaleString('en-US')}`}</p>}
      {!loading && !searchError && submittedArgs && results.length === 0 && <p className="search-status" data-testid="search-empty" role="status">{t.empty}</p>}
      {results.length > 0 && <section className="search-results" data-testid="search-results" aria-label={t.title} aria-live="polite">
        {results.map((item) => {
          const kindLabel = t[item.kind]
          const parentName = item.kind === 'post' || item.kind === 'response'
            ? item.name
            : item.board_id ? boardNames.get(item.board_id) : null
          const title = resultTitle(item, t)
          return <button className="search-result" data-testid={`search-result-${item.kind}-${item.id}`} type="button" key={resultKey(item)} onClick={() => selectResult(item)} disabled={blocked}>
            <span className="search-result-heading"><strong>{title}</strong><span>{kindLabel}</span></span>
            {parentName && <span className="search-result-parent">{parentName}</span>}
            <span className="search-result-excerpt">{item.excerpt}</span>
            <time dateTime={item.created_at}>{displayDate(item.created_at, language)}</time>
          </button>
        })}
      </section>}
      {moreError && <div className="search-error-block"><p className="search-error" data-testid="search-load-more-error" role="alert">{t.moreError}</p>{nextCursor && <button className="button button-secondary" data-testid="search-load-more-retry" type="button" onClick={() => void loadMore()} disabled={blocked || loadingMore}>{t.retryMore}</button>}</div>}
      {nextCursor && <div className="search-actions"><button className="button button-secondary" data-testid="search-load-more" type="button" onClick={() => void loadMore()} disabled={blocked || loadingMore || loading}>{loadingMore ? t.loadingMore : t.more}</button></div>}
      </div>
      <footer className="search-dialog-footer"><button className="button button-secondary" data-testid="search-close" type="button" onClick={onClose} disabled={blocked}>{t.close}</button></footer>
    </section>
  </div>
}
