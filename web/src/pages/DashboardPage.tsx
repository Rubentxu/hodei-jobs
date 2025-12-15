import { useJobs } from '@/hooks/useJobs'
import { useSystemMetrics } from '@/hooks/useMetrics'
import { formatDistanceToNow } from 'date-fns'
import { Link } from 'react-router-dom'

export function DashboardPage() {
  const { data: metrics, isLoading: loadingMetrics } = useSystemMetrics()
  const { data: recentJobs, isLoading: loadingJobs } = useJobs({ limit: 5, offset: 0 })

  // Map JobStatus (string from useJobs) to UI status
  const getUiStatus = (status: string): 'running' | 'success' | 'failed' => {
    switch (status) {
      case 'RUNNING':
      case 'PENDING':
      case 'ASSIGNED': // Added ASSIGNED
        return 'running'
      case 'SUCCEEDED': // useJobs uses SUCCEEDED, not COMPLETED
        return 'success'
      case 'FAILED':
      case 'CANCELLED':
      case 'TIMEOUT':
        return 'failed'
      default:
        return 'running' // Default fallback
    }
  }

  const formatTime = (date?: Date) => {
    if (!date) return '-'
    try {
      return formatDistanceToNow(date, { addSuffix: true })
    } catch (e) {
      return '-'
    }
  }

  return (
    <>
      {/* Header */}
      <header className="sticky top-0 z-20 flex items-center justify-between px-4 py-3 bg-background-light/80 dark:bg-background-dark/80 backdrop-blur-md border-b border-gray-200 dark:border-border-dark">
        <div className="flex items-center gap-3">
          <div className="w-10 h-10 rounded-full bg-gradient-to-br from-primary to-blue-400 flex items-center justify-center text-white font-bold ring-2 ring-primary/20">
            A
          </div>
          <div>
            <p className="text-sm font-semibold text-gray-900 dark:text-white">Admin User</p>
            <p className="text-xs text-gray-500 dark:text-gray-400">Platform Admin</p>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <button className="p-2 text-gray-500 dark:text-gray-400 hover:text-primary transition-colors">
            <span className="material-symbols-outlined">search</span>
          </button>
          <button className="p-2 text-gray-500 dark:text-gray-400 hover:text-primary transition-colors relative">
            <span className="material-symbols-outlined">notifications</span>
            <span className="absolute top-1 right-1 w-2 h-2 bg-red-500 rounded-full"></span>
          </button>
        </div>
      </header>

      {/* Stats Grid */}
      <div className="grid grid-cols-2 gap-3 p-4">
        <StatsCard
          icon="dataset"
          label="Total Jobs"
          value={loadingMetrics ? '...' : metrics?.totalJobs.toLocaleString() || '0'}
        />
        <StatsCard
          icon="sync"
          label="Running"
          value={loadingMetrics ? '...' : metrics?.runningJobs.toLocaleString() || '0'}
          variant="primary"
          spinning={metrics?.runningJobs ? metrics.runningJobs > 0 : false}
        />
        <StatsCard
          icon="error"
          label="Failed"
          value={loadingMetrics ? '...' : metrics?.failedJobs.toLocaleString() || '0'}
          variant="error"
        />
        <StatsCard
          icon="check_circle"
          label="Success"
          value={loadingMetrics ? '...' : metrics?.succeededJobs.toLocaleString() || '0'}
          variant="success"
        />
      </div>

      {/* System Health */}
      <div className="px-4 mb-4">
        <div className="bg-white dark:bg-card-dark rounded-xl border border-gray-200 dark:border-border-dark p-4">
          <div className="flex justify-between items-start mb-4">
            <div>
              <h3 className="text-base font-bold text-gray-900 dark:text-white">System Health</h3>
              <p className="text-xs text-gray-500 dark:text-gray-400">CPU Load (24h)</p>
            </div>
            <span className="text-lg font-bold text-success">
              {loadingMetrics ? '...' : `${metrics?.cpuLoad.toFixed(1) || 0}%`}
            </span>
          </div>

          {/* SVG Chart placeholder */}
          <div className="h-24 mb-4 relative">
            <svg className="w-full h-full" viewBox="0 0 300 80" preserveAspectRatio="none">
              <defs>
                <linearGradient id="chartGradient" x1="0%" y1="0%" x2="0%" y2="100%">
                  <stop offset="0%" stopColor="#2b6cee" stopOpacity="0.3" />
                  <stop offset="100%" stopColor="#2b6cee" stopOpacity="0" />
                </linearGradient>
              </defs>
              <path
                d="M0,60 Q30,40 60,50 T120,30 T180,45 T240,25 T300,35 L300,80 L0,80 Z"
                fill="url(#chartGradient)"
              />
              <path
                d="M0,60 Q30,40 60,50 T120,30 T180,45 T240,25 T300,35"
                fill="none"
                stroke="#2b6cee"
                strokeWidth="2"
              />
            </svg>
          </div>

          <div className="flex justify-between items-center pt-3 border-t border-gray-100 dark:border-border-dark">
            <div className="flex items-center gap-2">
              <span className="relative flex h-2 w-2">
                <span className={`animate-ping absolute inline-flex h-full w-full rounded-full ${loadingMetrics ? 'bg-gray-400' : 'bg-emerald-400'} opacity-75`}></span>
                <span className={`relative inline-flex rounded-full h-2 w-2 ${loadingMetrics ? 'bg-gray-500' : 'bg-emerald-500'}`}></span>
              </span>
              <span className="text-xs text-gray-600 dark:text-gray-300">
                {loadingMetrics ? '...' : `${metrics?.activeNodes || 0} Online`}
              </span>
            </div>
            <div className="flex items-center gap-2">
              <span className="material-symbols-outlined text-amber-500 text-sm">schedule</span>
              <span className="text-xs text-gray-600 dark:text-gray-300">
                Queue: {loadingMetrics ? '...' : metrics?.queueSize || 0}
              </span>
            </div>
          </div>
        </div>
      </div>

      {/* Recent Executions */}
      <div className="px-4">
        <div className="flex justify-between items-center mb-3">
          <h3 className="text-base font-bold text-gray-900 dark:text-white">Recent Executions</h3>
          <Link to="/jobs" className="text-xs text-primary font-medium">See All</Link>
        </div>

        <div className="flex flex-col gap-3">
          {loadingJobs ? (
            // Loading Skeleton
            [1, 2, 3].map(i => (
              <div key={i} className="h-16 bg-gray-100 dark:bg-card-dark rounded-xl animate-pulse"></div>
            ))
          ) : recentJobs?.length === 0 ? (
            <div className="text-center py-4 text-gray-500 text-xs">No recent jobs</div>
          ) : (
            recentJobs?.map(job => (
              <RecentJobCard
                key={job.id}
                id={job.id}
                name={job.name}
                status={getUiStatus(job.status)}
                time={formatTime(job.startedAtRaw)}
              />
            ))
          )}
        </div>
      </div>

      {/* FAB */}
      <Link
        to="/jobs/new"
        className="fixed bottom-24 right-4 z-20 w-14 h-14 bg-primary rounded-full shadow-lg shadow-blue-500/40 flex items-center justify-center text-white hover:scale-105 active:scale-95 transition-transform"
      >
        <span className="material-symbols-outlined text-2xl">add</span>
      </Link>
    </>
  )
}

interface StatsCardProps {
  icon: string
  label: string
  value: string
  variant?: 'default' | 'primary' | 'error' | 'success'
  spinning?: boolean
}

function StatsCard({ icon, label, value, variant = 'default', spinning }: StatsCardProps) {
  const colorClasses = {
    default: 'text-gray-600 dark:text-gray-300',
    primary: 'text-primary',
    error: 'text-error',
    success: 'text-success',
  }

  return (
    <div className="bg-white dark:bg-card-dark rounded-xl border border-gray-200 dark:border-border-dark p-4 hover:border-gray-300 dark:hover:border-gray-600 transition-colors">
      <div className="flex items-center gap-2 mb-2">
        <span className={`material-symbols-outlined text-lg ${colorClasses[variant]} ${spinning ? 'animate-spin' : ''}`}>
          {icon}
        </span>
        <span className="text-xs text-gray-500 dark:text-gray-400 font-medium">{label}</span>
      </div>
      <p className={`text-2xl font-bold ${colorClasses[variant]}`}>{value}</p>
    </div>
  )
}

interface RecentJobCardProps {
  id: string
  name: string
  status: 'running' | 'success' | 'failed'
  time: string
}

function RecentJobCard({ id, name, status, time }: RecentJobCardProps) {
  const statusConfig = {
    running: { icon: 'sync', color: 'text-primary', bg: 'bg-primary/10', spinning: true },
    success: { icon: 'check_circle', color: 'text-success', bg: 'bg-success/10', spinning: false },
    failed: { icon: 'error', color: 'text-error', bg: 'bg-error/10', spinning: false },
  }

  const config = statusConfig[status]

  return (
    <Link
      to={`/jobs/${id}`}
      className="flex items-center gap-3 p-3 bg-white dark:bg-card-dark rounded-xl border border-gray-200 dark:border-border-dark active:scale-[0.98] transition-transform"
    >
      <div className={`w-10 h-10 rounded-full ${config.bg} ${config.color} flex items-center justify-center`}>
        <span className={`material-symbols-outlined ${config.spinning ? 'animate-spin' : ''}`}>{config.icon}</span>
      </div>
      <div className="flex-1 min-w-0">
        <p className="text-sm font-semibold text-gray-900 dark:text-white truncate">Job #{id}</p>
        <p className="text-xs text-gray-500 dark:text-gray-400 truncate">{name}</p>
      </div>
      <div className="flex items-center gap-2">
        <span className="text-xs text-gray-400">{time}</span>
        {status !== 'running' && (
          <button className="p-1 text-gray-400 hover:text-primary transition-colors">
            <span className="material-symbols-outlined text-lg">replay</span>
          </button>
        )}
      </div>
    </Link>
  )
}
