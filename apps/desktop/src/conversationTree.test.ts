import { describe, expect, it } from 'vitest'
import { buildConversationTree, getConversationAncestors, visibleConversationRows } from './conversationTree'
import type { Post } from './types'

function post(no: number, replyTo: number | null, extra: Partial<Post> = {}): Post {
  return {
    id: `post-${no}`, thread_id: 'thread-a', no, body: `body ${no}`,
    author: { id: 'author-a', model_name: 'Model', display_id: 'AA11' },
    created_at: '2026-10-08T00:00:00Z', updated_at: '2026-10-08T00:00:00Z', revision: 1,
    reply_to: replyTo, edited_by: null, editor_display_id: null, editor_model_name: null,
    deleted: false, deleted_at: null, response_count: 0, ...extra,
  }
}

describe('conversation tree', () => {
  it('nests formal replies in stable preorder and preserves the source page', () => {
    const source = [post(4, 2), post(1, null, { response_count: 99 }), post(3, 2), post(2, 1)]
    const before = structuredClone(source)

    const rows = buildConversationTree(source)

    expect(rows.map(({ post: item, parentNo, depth, descendantCount }) => [item.no, parentNo, depth, descendantCount])).toEqual([
      [1, null, 0, 3], [2, 1, 1, 2], [3, 2, 2, 0], [4, 2, 2, 0],
    ])
    expect(source).toEqual(before)
  })

  it('keeps missing and deleted parents visible without changing the reply Post No', () => {
    const rows = buildConversationTree([
      post(8, 5),
      post(5, null, { deleted: true, deleted_at: '2026-10-08T01:00:00Z', body: null }),
      post(9, 77),
    ])

    expect(rows.map(({ post: item, parentNo, depth, parentOutsidePage }) => ({ no: item.no, replyTo: item.reply_to, parentNo, depth, parentOutsidePage, deleted: item.deleted }))).toEqual([
      { no: 5, replyTo: null, parentNo: null, depth: 0, parentOutsidePage: false, deleted: true },
      { no: 8, replyTo: 5, parentNo: 5, depth: 1, parentOutsidePage: false, deleted: false },
      { no: 9, replyTo: 77, parentNo: null, depth: 0, parentOutsidePage: true, deleted: false },
    ])
    expect(rows.find(row => row.post.no === 8)?.post.no).toBe(8)
  })

  it('handles a 1,000-Post chain iteratively and finds its ancestor path', () => {
    const source = Array.from({ length: 1000 }, (_, index) => post(index + 1, index === 0 ? null : index))
    const rows = buildConversationTree(source)

    expect(rows).toHaveLength(1000)
    expect(rows[999]).toMatchObject({ depth: 999, descendantCount: 0, parentNo: 999 })
    expect(getConversationAncestors(rows, 1000)).toHaveLength(999)
    expect(getConversationAncestors(rows, 1000).slice(0, 2)).toEqual([1, 2])
  })

  it('breaks corrupt cycles and invalid cross-thread/future parents deterministically without dropping Posts', () => {
    const source = [
      post(1, 2), // cycle/future edge is rejected
      post(2, 1),
      post(3, 1, { thread_id: 'thread-b' }),
      post(4, 5),
      post(5, null),
    ]
    const rows = buildConversationTree(source)

    expect(rows.map(row => row.post.no)).toEqual([1, 2, 3, 4, 5])
    expect(new Set(rows.map(row => row.post.no)).size).toBe(source.length)
    expect(rows.find(row => row.post.no === 1)).toMatchObject({ parentNo: null, depth: 0, parentOutsidePage: true })
    expect(rows.find(row => row.post.no === 2)).toMatchObject({ parentNo: 1, depth: 1 })
    expect(buildConversationTree(source).map(row => [row.post.no, row.parentNo, row.depth])).toEqual(rows.map(row => [row.post.no, row.parentNo, row.depth]))
  })

  it('collapses exactly the loaded descendants and leaves adjacent roots visible', () => {
    const rows = buildConversationTree([post(1, null), post(2, 1), post(3, 2), post(4, 1), post(5, null), post(6, 5)])

    expect(rows.find(row => row.post.no === 1)?.descendantCount).toBe(3)
    expect(visibleConversationRows(rows, new Set([1])).map(row => row.post.no)).toEqual([1, 5, 6])
    expect(visibleConversationRows(rows, new Set([2])).map(row => row.post.no)).toEqual([1, 2, 4, 5, 6])
  })
})
