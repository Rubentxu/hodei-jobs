import { Link } from 'react-router-dom'
import { clsx } from 'clsx'

export type JobStatus = 'RUNNING' | 'FAILED' | 'SUCCEEDED' | 'PENDING'

interface JobCardProps {
  id: string
  name: string
  status: JobStatus
  startedAt: string
  duration: string
  progress?: number
  isOld?: boolean
}

const statusConfig: Record<JobStatus, { icon: string; bgColor: string; textColor: string; label: string }> = {
  RUNNING: {
    icon: 'sync',
    bgColor: 'bg-primary/10',
    textColor: 'text-primary',
    label: 'Running',
  },
  FAILED: {
    icon: 'error',
    bgColor: 'bg-error/10',
    textColor: 'text-error',
    label: 'Failed',
  },
  SUCCEEDED: {
    icon: 'check_circle',
    bgColor: 'bg-success/10',
    textColor: 'text-success',
    label: 'Succeeded',
  },
  PENDING: {
    icon: 'schedule',
    bgColor: 'bg-gray-100 dark:bg-gray-800',
    textColor: 'text-gray-500 dark:text-gray-400',
    label: 'Pending',
  },
}

export function JobCard({ id, name, status, startedAt, duration, progress, isOld }: JobCardProps) {
  const config = statusConfig[status]

  return (
    <Link
      to={`/jobs/${id}`}
      className={clsx(
        'group flex flex-col bg-white dark:bg-card-dark rounded-xl border border-gray-200 dark:border-border-dark shadow-sm active:scale-[0.99] transition-transform duration-100',
        isOld && 'opacity-80'
      )}
    >
      <div className="flex items-center p-4 gap-4">
        {/* Status Icon */}
        <div
          className={clsx(
            'relative flex items-center justify-center shrink-0 w-10 h-10 rounded-full',
            config.bgColor,
            config.textColor
          )}
        >
          <span className="material-symbols-outlined text-[24px]">{config.icon}</span>
          {status === 'RUNNING' && (
            <div className="absolute inset-0 rounded-full border-2 border-primary/20 border-t-primary animate-spin" />
          )}
        </div>

        {/* Content */}
        <div className="flex flex-1 flex-col min-w-0">
          <div className="flex justify-between items-start">
            <h3 className="text-base font-semibold text-gray-900 dark:text-white truncate">
              Job #{id}
            </h3>
            <span
              className={clsx(
                'inline-flex items-center rounded-md px-2 py-1 text-xs font-medium ring-1 ring-inset',
                config.bgColor,
                config.textColor,
                status === 'RUNNING' && 'ring-primary/20',
                status === 'FAILED' && 'ring-error/20',
                status === 'SUCCEEDED' && 'ring-success/20',
                status === 'PENDING' && 'ring-gray-500/10'
              )}
            >
              {config.label}
            </span>
          </div>
          <p className="text-sm text-gray-500 dark:text-gray-400 truncate mt-0.5">{name}</p>
          <div className="flex items-center gap-3 mt-2 text-xs text-gray-400">
            <div className="flex items-center gap-1">
              <span className="material-symbols-outlined text-[14px]">schedule</span>
              {startedAt}
            </div>
            <div className="flex items-center gap-1">
              <span className="material-symbols-outlined text-[14px]">timer</span>
              {duration}
            </div>
          </div>
        </div>

        {/* Chevron */}
        <div className="shrink-0 text-gray-300 dark:text-gray-600">
          <span className="material-symbols-outlined">chevron_right</span>
        </div>
      </div>

      {/* Progress bar for running job */}
      {status === 'RUNNING' && progress !== undefined && (
        <div className="h-1 w-full bg-gray-100 dark:bg-border-dark rounded-b-xl overflow-hidden">
          <div className="h-full bg-primary" style={{ width: `${progress}%` }} />
        </div>
      )}
    </Link>
  )
}
