import { describe, expect, it, vi } from 'vitest'
import { readAllPages } from './readAllPages'

describe('complete conversation lists', () => {
  it('follows opaque cursors without losing later Boards or Threads', async () => {
    const read = vi.fn().mockResolvedValueOnce({ items: Array.from({ length: 200 }, (_, i) => i), next_cursor: 'opaque' }).mockResolvedValueOnce({ items: [200], next_cursor: null })
    expect((await readAllPages<number>(read)).items).toHaveLength(201)
    expect(read.mock.calls).toEqual([[{ limit: 200 }], [{ limit: 200, cursor: 'opaque' }]])
  })
  it('drops every accumulated item when the account/request changes during paging', async () => {
    let current = true
    const read = vi.fn().mockResolvedValueOnce({ items: ['private'], next_cursor: 'next' }).mockImplementationOnce(async () => { current = false; return { items: ['also-private'], next_cursor: null } })
    expect(await readAllPages(read, () => current)).toEqual({ items: [] })
  })
  it('fails closed on a repeated token or a permission/cursor error', async () => {
    const read = vi.fn().mockResolvedValue({ items: [], next_cursor: 'repeated' })
    await expect(readAllPages(read)).rejects.toThrow('refresh required')
    expect(read).toHaveBeenCalledTimes(2)
    await expect(readAllPages(async () => { throw new Error('RESYNC_REQUIRED') })).rejects.toThrow('RESYNC_REQUIRED')
  })
})
