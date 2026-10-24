// A stale generation must never publish a partially read private tree.
export async function readAllPages<T>(
  read: (args: { limit: number; cursor?: string }) => Promise<{ items: T[]; next_cursor?: string | null }>,
  isCurrent: () => boolean = () => true,
): Promise<{ items: T[] }> {
  const items: T[] = []
  const seen = new Set<string>()
  let cursor: string | undefined
  do {
    if (!isCurrent()) return { items: [] }
    const page = await read({ limit: 200, ...(cursor ? { cursor } : {}) })
    if (!isCurrent()) return { items: [] }
    items.push(...page.items)
    cursor = page.next_cursor || undefined
    if (cursor && seen.has(cursor)) throw new Error('List cursor repeated; refresh required')
    if (cursor) seen.add(cursor)
  } while (cursor)
  return { items }
}
