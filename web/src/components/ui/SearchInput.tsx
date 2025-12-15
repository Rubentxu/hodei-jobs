interface SearchInputProps {
  placeholder?: string
  value: string
  onChange: (value: string) => void
}

export function SearchInput({ placeholder = 'Search...', value, onChange }: SearchInputProps) {
  return (
    <div className="px-4 py-3 sticky top-[57px] z-10 bg-background-light dark:bg-background-dark">
      <div className="relative w-full group">
        <div className="absolute inset-y-0 left-0 flex items-center pl-3 pointer-events-none text-gray-400">
          <span className="material-symbols-outlined text-[20px]">search</span>
        </div>
        <input
          type="text"
          value={value}
          onChange={(e) => onChange(e.target.value)}
          className="block w-full p-2.5 pl-10 text-sm text-gray-900 bg-white border border-gray-200 rounded-xl focus:ring-primary focus:border-primary dark:bg-card-dark dark:border-border-dark dark:placeholder-gray-500 dark:text-white dark:focus:ring-primary/50 dark:focus:border-primary transition-all shadow-sm"
          placeholder={placeholder}
        />
      </div>
    </div>
  )
}
