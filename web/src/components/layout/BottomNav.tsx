import { NavLink } from 'react-router-dom'
import { clsx } from 'clsx'

interface NavItem {
  to: string
  icon: string
  label: string
}

const navItems: NavItem[] = [
  { to: '/', icon: 'dashboard', label: 'Overview' },
  { to: '/jobs', icon: 'history', label: 'Jobs' },
  { to: '/providers', icon: 'dns', label: 'Providers' },
  { to: '/metrics', icon: 'monitoring', label: 'Metrics' },
]

export function BottomNav() {
  return (
    <div className="fixed bottom-0 left-0 right-0 z-30 bg-white dark:bg-[#161b26] border-t border-gray-200 dark:border-border-dark px-6 py-2 pb-6 flex justify-between items-center shadow-[0_-4px_6px_-1px_rgba(0,0,0,0.1)] dark:shadow-none">
      {navItems.map((item) => (
        <NavLink
          key={item.to}
          to={item.to}
          end={item.to === '/'}
          className={({ isActive }) =>
            clsx(
              'flex flex-col items-center justify-center gap-1 p-2 transition-colors',
              isActive
                ? 'text-primary'
                : 'text-gray-400 hover:text-gray-600 dark:hover:text-gray-300'
            )
          }
        >
          {({ isActive }) => (
            <>
              <span
                className={clsx(
                  'material-symbols-outlined text-[26px]',
                  isActive && 'filled'
                )}
              >
                {item.icon}
              </span>
              <span className="text-[10px] font-medium">{item.label}</span>
            </>
          )}
        </NavLink>
      ))}
    </div>
  )
}
