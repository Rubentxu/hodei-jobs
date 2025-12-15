import { clsx } from 'clsx'

interface HeaderProps {
  title: string
  subtitle?: string
  showBack?: boolean
  showSearch?: boolean
  showNotifications?: boolean
  showFilter?: boolean
  rightAction?: React.ReactNode
  onBack?: () => void
  className?: string
}

export function Header({
  title,
  subtitle,
  showBack = false,
  showSearch = false,
  showNotifications = false,
  showFilter = false,
  rightAction,
  onBack,
  className,
}: HeaderProps) {
  return (
    <header
      className={clsx(
        'sticky top-0 z-20 flex items-center justify-between px-4 py-3',
        'bg-background-light/95 dark:bg-background-dark/95 backdrop-blur-md',
        'border-b border-gray-200 dark:border-border-dark',
        className
      )}
    >
      <div className="w-8 flex justify-start">
        {showBack && (
          <button
            onClick={onBack}
            className="text-gray-500 dark:text-gray-400 hover:text-primary transition-colors"
          >
            <span className="material-symbols-outlined">arrow_back_ios_new</span>
          </button>
        )}
      </div>

      <div className="flex flex-col items-center flex-1">
        <h1 className="text-lg font-bold tracking-tight text-center">{title}</h1>
        {subtitle && (
          <span className="text-xs text-gray-500 dark:text-gray-400 font-medium">
            {subtitle}
          </span>
        )}
      </div>

      <div className="w-8 flex justify-end gap-2">
        {showSearch && (
          <button className="text-gray-500 dark:text-gray-400 hover:text-primary transition-colors">
            <span className="material-symbols-outlined text-2xl">search</span>
          </button>
        )}
        {showNotifications && (
          <button className="text-gray-500 dark:text-gray-400 hover:text-primary transition-colors">
            <span className="material-symbols-outlined text-2xl">notifications</span>
          </button>
        )}
        {showFilter && (
          <button className="text-gray-500 dark:text-gray-400 hover:text-primary transition-colors">
            <span className="material-symbols-outlined text-2xl">filter_list</span>
          </button>
        )}
        {rightAction}
      </div>
    </header>
  )
}
