import { beforeEach, describe, expect, it } from 'vitest'
import { clearOperationStatuses, getOperationStatuses, operationStatusLabel, trackOperation } from './operationStatus'

beforeEach(clearOperationStatuses)
describe('operation status log', () => {
  it('shows concurrent reads and posting until each actual request settles', async () => {
    let finishRead!: (value: number) => void
    let finishPost!: (value: string) => void
    const read = trackOperation('thread.list', () => new Promise<number>((resolve) => { finishRead = resolve }))
    const post = trackOperation('post.create', () => new Promise<string>((resolve) => { finishPost = resolve }))
    expect(getOperationStatuses().map((entry) => operationStatusLabel(entry, 'ja'))).toEqual(['Threadsを読み込み中', 'Postを投稿中'])
    finishRead(3)
    expect(await read).toBe(3)
    expect(getOperationStatuses().find((entry) => entry.operation === 'post.create')?.state).toBe('pending')
    finishPost('private returned body')
    expect(await post).toBe('private returned body')
    expect(getOperationStatuses().every((entry) => entry.state === 'completed')).toBe(true)
    expect(JSON.stringify(getOperationStatuses())).not.toContain('private returned body')
  })
  it('preserves rejection and records failure without error details', async () => {
    const error = new Error('private credential')
    await expect(trackOperation('post.create', async () => { throw error })).rejects.toBe(error)
    expect(getOperationStatuses()[0].state).toBe('failed')
    expect(JSON.stringify(getOperationStatuses())).not.toContain('private credential')
  })
  it('ignores completion from a cleared account session', async () => {
    let finish!: () => void
    const pending = trackOperation('thread.list', () => new Promise<void>((resolve) => { finish = resolve }))
    clearOperationStatuses()
    finish()
    await pending
    expect(getOperationStatuses()).toEqual([])
  })
  it('bounds completed history while retaining active requests', async () => {
    let finish!: () => void
    const pending = trackOperation('post.create', () => new Promise<void>((resolve) => { finish = resolve }))
    for (let index = 0; index < 120; index++) await trackOperation(`operation-${index}`, async () => index)
    expect(getOperationStatuses().filter((entry) => entry.state === 'completed')).toHaveLength(100)
    expect(getOperationStatuses().filter((entry) => entry.state === 'pending')).toHaveLength(1)
    finish(); await pending
  })
})
