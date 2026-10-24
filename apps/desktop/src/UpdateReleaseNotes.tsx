import type { UpdateNotes } from './updates'

export default function UpdateReleaseNotes({ notes, language }: { notes: UpdateNotes; language: 'ja' | 'en' }) {
  return <section className="update-release-notes" aria-labelledby="update-notes-title">
    <h3 id="update-notes-title">{language === 'ja' ? 'アップデート内容' : 'What’s new'}</h3>
    <p>{language === 'ja' ? '更新済みバージョン' : 'Updated version'} {notes.version}</p>
    <div className="update-notes-body">{notes.notes}</div>
  </section>
}
