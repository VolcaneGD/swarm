// @vitest-environment jsdom
import { act } from 'react'
import { createRoot } from 'react-dom/client'
import { expect, it } from 'vitest'
import UpdateReleaseNotes from './UpdateReleaseNotes'

it('shows the installed version and treats release text as text, never executable markup', async () => {
  const host = document.createElement('div')
  const root = createRoot(host)
  await act(async () => root.render(<UpdateReleaseNotes language="ja" notes={{ version: '0.2.0', notes: '改善点\n<script>alert(1)</script>' }} />))
  expect(host.textContent).toContain('アップデート内容')
  expect(host.textContent).toContain('更新済みバージョン 0.2.0')
  expect(host.textContent).toContain('<script>alert(1)</script>')
  expect(host.querySelector('script')).toBeNull()
  await act(async () => root.unmount())
})
