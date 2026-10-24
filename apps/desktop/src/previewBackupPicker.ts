// Browser preview only: choose a name without reading or uploading file contents.
// Browsers do not permit selecting a native initial directory.
export function pickPreviewBackupFile(): Promise<string | null> {
  return new Promise((resolve, reject) => {
    const input = document.createElement('input')
    input.type = 'file'
    input.accept = '.backup'
    input.hidden = true
    const cleanup = () => input.remove()
    input.addEventListener('change', () => {
      const name = input.files?.[0]?.name ?? null
      cleanup()
      if (name && !name.toLowerCase().endsWith('.backup')) reject(new Error('Choose a .backup file'))
      else resolve(name)
    }, { once: true })
    input.addEventListener('cancel', () => { cleanup(); resolve(null) }, { once: true })
    document.body.append(input)
    try { input.click() }
    catch (error) { cleanup(); reject(error) }
  })
}
