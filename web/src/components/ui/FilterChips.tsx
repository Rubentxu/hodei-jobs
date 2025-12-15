import { clsx } from 'clsx'

export interface FilterChip {
  id: string
  label: string
  color?: string
}

interface FilterChipsProps {
  chips: FilterChip[]
  activeId: string
  onChange: (id: string) => void
}

export function FilterChips({ chips, activeId, onChange }: FilterChipsProps) {
  return (
    <div className="flex gap-2 px-4 pb-2 overflow-x-auto scrollbar-hide snap-x">
      {chips.map((chip) => (
        <button
          key={chip.id}
          onClick={() => onChange(chip.id)}
          className={clsx(
            'flex items-center justify-center px-4 py-1.5 rounded-full text-sm font-medium whitespace-nowrap snap-start transition-colors',
            activeId === chip.id
              ? 'bg-primary text-white shadow-sm shadow-primary/20'
              : 'bg-white dark:bg-card-dark border border-gray-200 dark:border-border-dark text-gray-600 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-border-dark'
          )}
        >
          {chip.color && (
            <span
              className="w-2 h-2 rounded-full mr-2"
              style={{ backgroundColor: chip.color }}
            />
          )}
          {chip.label}
        </button>
      ))}
    </div>
  )
}
