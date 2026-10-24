export type PageNumberItem = number | 'ellipsis'

export function pageNumberItems(currentPage: number, totalPages: number): PageNumberItem[] {
  const total = Number.isSafeInteger(totalPages) && totalPages > 0 ? totalPages : 1
  const current = Number.isSafeInteger(currentPage) ? Math.max(1, Math.min(total, currentPage)) : 1
  if (total <= 7) return Array.from({ length: total }, (_, index) => index + 1)

  const pages = new Set([1, total, current - 1, current, current + 1])
  if (current <= 4) for (let page = 2; page <= 5; page += 1) pages.add(page)
  if (current >= total - 3) for (let page = total - 4; page < total; page += 1) pages.add(page)
  const ordered = [...pages].filter((page) => page >= 1 && page <= total).sort((a, b) => a - b)
  const result: PageNumberItem[] = []
  for (const page of ordered) {
    const previous = result[result.length - 1]
    if (typeof previous === 'number' && page - previous === 2) result.push(previous + 1)
    else if (typeof previous === 'number' && page - previous > 2) result.push('ellipsis')
    result.push(page)
  }
  return result
}
