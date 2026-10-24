import type { Post } from './types'

export type ConversationRow = {
  post: Post
  parentNo: number | null
  depth: number
  descendantCount: number
  parentOutsidePage: boolean
}

/** Group this authorized numbered page without fetching or inventing parent data. */
export function buildConversationTree(posts: readonly Post[]): ConversationRow[] {
  const byNo = new Map<number, Post>()
  for (const post of [...posts].sort((a, b) => a.no - b.no)) {
    if (!byNo.has(post.no)) byNo.set(post.no, post)
  }
  const children = new Map<number, number[]>()
  const roots: number[] = []
  const parents = new Map<number, number | null>()
  for (const post of byNo.values()) {
    const candidate = post.reply_to === null ? undefined : byNo.get(post.reply_to)
    // Stored replies point to earlier numbers in the same Thread. Reject broken
    // relations defensively so every supplied Post remains visible exactly once.
    const parent = candidate && candidate.no < post.no && candidate.thread_id === post.thread_id ? candidate.no : null
    parents.set(post.no, parent)
    if (parent === null) roots.push(post.no)
    else {
      const siblings = children.get(parent) ?? []
      siblings.push(post.no)
      children.set(parent, siblings)
    }
  }
  const rows: ConversationRow[] = []
  const stack = roots.slice().reverse().map(no => ({ no, depth: 0 }))
  while (stack.length) {
    const { no, depth } = stack.pop()!
    const post = byNo.get(no)!
    const parentNo = parents.get(no) ?? null
    rows.push({ post, parentNo, depth, descendantCount: 0, parentOutsidePage: post.reply_to !== null && parentNo === null })
    const replies = children.get(no) ?? []
    for (let index = replies.length - 1; index >= 0; index -= 1) stack.push({ no: replies[index], depth: depth + 1 })
  }
  const rowByNo = new Map(rows.map(row => [row.post.no, row]))
  for (let index = rows.length - 1; index >= 0; index -= 1) {
    const row = rows[index]
    if (row.parentNo !== null) rowByNo.get(row.parentNo)!.descendantCount += row.descendantCount + 1
  }
  return rows
}

export function visibleConversationRows(rows: readonly ConversationRow[], collapsed: ReadonlySet<number>): ConversationRow[] {
  let hiddenBelow: number | null = null
  return rows.filter(row => {
    if (hiddenBelow !== null && row.depth > hiddenBelow) return false
    hiddenBelow = null
    if (collapsed.has(row.post.no) && row.descendantCount > 0) hiddenBelow = row.depth
    return true
  })
}

export function getConversationAncestors(rows: readonly ConversationRow[], targetNo: number): number[] {
  const byNo = new Map(rows.map(row => [row.post.no, row]))
  const ancestors: number[] = []
  let row = byNo.get(targetNo)
  while (row?.parentNo !== null && row?.parentNo !== undefined) {
    ancestors.push(row.parentNo)
    row = byNo.get(row.parentNo)
  }
  return ancestors.reverse()
}
