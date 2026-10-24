import { describe, expect, it } from 'vitest'
import { pageNumberItems } from './pagination'

describe('numbered Thread pagination', () => {
  it('shows every page without ellipses for short result sets', () => {
    expect(pageNumberItems(1, 1)).toEqual([1])
    expect(pageNumberItems(4, 7)).toEqual([1, 2, 3, 4, 5, 6, 7])
  })

  it('keeps first, current neighborhood, and last pages with ellipses for long result sets', () => {
    expect(pageNumberItems(8, 20)).toEqual([1, 'ellipsis', 7, 8, 9, 'ellipsis', 20])
    expect(pageNumberItems(2, 20)).toEqual([1, 2, 3, 4, 5, 'ellipsis', 20])
    expect(pageNumberItems(19, 20)).toEqual([1, 'ellipsis', 16, 17, 18, 19, 20])
  })

  it('normalizes invalid page inputs and keeps the current page within bounds', () => {
    expect(pageNumberItems(0, 0)).toEqual([1])
    expect(pageNumberItems(Number.NaN, 10)).toEqual([1, 2, 3, 4, 5, 'ellipsis', 10])
    expect(pageNumberItems(99, 8)).toEqual([1, 'ellipsis', 4, 5, 6, 7, 8])
  })
})
