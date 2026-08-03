import { useEffect, useRef, useState } from 'react'
import { CaretDown, Check } from '@phosphor-icons/react'

export interface Option {
  value: string
  label: string
}

interface Props {
  id?: string
  value: string
  options: Option[]
  onChange: (value: string) => void
}

/** A dropdown we can actually style — the native popup is system chrome that
    ignores the rest of the design. */
export default function Select({ id, value, options, onChange }: Props) {
  const [open, setOpen] = useState(false)
  const root = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return
    const away = (e: MouseEvent) => {
      if (!root.current?.contains(e.target as Node)) setOpen(false)
    }
    const esc = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation()
        setOpen(false)
      }
    }
    document.addEventListener('mousedown', away)
    document.addEventListener('keydown', esc, true)
    return () => {
      document.removeEventListener('mousedown', away)
      document.removeEventListener('keydown', esc, true)
    }
  }, [open])

  const current = options.find((o) => o.value === value)

  return (
    <div className="select" ref={root}>
      <button
        id={id}
        type="button"
        className={`select-trigger${open ? ' open' : ''}`}
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
      >
        <span className="select-value">{current?.label ?? ''}</span>
        <CaretDown weight="bold" />
      </button>

      {open ? (
        <div className="select-menu" role="listbox">
          {options.map((option) => (
            <button
              key={option.value}
              type="button"
              role="option"
              aria-selected={option.value === value}
              className={`select-option${option.value === value ? ' on' : ''}`}
              onClick={() => {
                onChange(option.value)
                setOpen(false)
              }}
            >
              <span>{option.label}</span>
              {option.value === value ? <Check weight="bold" /> : null}
            </button>
          ))}
        </div>
      ) : null}
    </div>
  )
}
