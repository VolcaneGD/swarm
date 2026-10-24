import { useId, type InputHTMLAttributes } from 'react'

// Suggestions are names remembered by this data root, never credentials or an account directory.
export default function AccountNameInput({ history = [], ...props }: InputHTMLAttributes<HTMLInputElement> & { history?: string[] }) {
  const id = useId()
  const names = [...new Set(history.filter(name => typeof name === 'string' && name.trim() && name.length <= 4096))].slice(0, 20)
  return <><input {...props} autoComplete="off" list={names.length ? id : undefined} />
    {names.length > 0 && <datalist id={id}>{names.map(name => <option key={name} value={name} />)}</datalist>}
  </>
}
