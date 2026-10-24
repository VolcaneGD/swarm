import type { KeyboardEvent } from 'react'

export default function Tabs<T extends string>({ id, label, items, value, onChange, disabled = false }: {
  id: string
  label: string
  items: readonly { value: T; label: string }[]
  value: T
  onChange: (value: T) => void
  disabled?: boolean
}) {
  function move(event: KeyboardEvent<HTMLButtonElement>, index: number) {
    const direction = event.key === 'ArrowRight' ? 1 : event.key === 'ArrowLeft' ? -1 : 0
    if (!direction && event.key !== 'Home' && event.key !== 'End') return
    event.preventDefault()
    const next = event.key === 'Home' ? 0 : event.key === 'End' ? items.length - 1 : (index + direction + items.length) % items.length
    onChange(items[next].value)
    event.currentTarget.parentElement?.querySelectorAll<HTMLButtonElement>('[role="tab"]')[next]?.focus()
  }
  return <div className="dialog-tabs" role="tablist" aria-label={label}>
    {items.map((item, index) => <button key={item.value} id={`${id}-tab-${item.value}`} type="button" role="tab"
      aria-selected={value === item.value} aria-controls={`${id}-panel-${item.value}`} tabIndex={value === item.value ? 0 : -1}
      disabled={disabled} onClick={() => onChange(item.value)} onKeyDown={(event) => move(event, index)}>{item.label}</button>)}
  </div>
}
