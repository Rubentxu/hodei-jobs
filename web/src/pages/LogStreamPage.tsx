import { useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'

type LogLevel = 'INFO' | 'WARN' | 'ERROR'

interface LogEntry {
  timestamp: string
  level: LogLevel
  message: string
}

const mockLogs: LogEntry[] = [
  { timestamp: '10:42:01', level: 'INFO', message: 'Initializing build environment...' },
  { timestamp: '10:42:02', level: 'INFO', message: 'Checking out source code from <span class="text-primary underline cursor-pointer">git@github.com:org/repo.git</span>' },
  { timestamp: '10:42:02', level: 'INFO', message: 'Git commit: <span class="text-yellow-200">7f8a9d2</span> (main)' },
  { timestamp: '10:42:04', level: 'INFO', message: 'Restoring npm cache...' },
  { timestamp: '10:42:08', level: 'WARN', message: "Package 'request' is deprecated. Please migrate to 'axios' or 'fetch'." },
  { timestamp: '10:42:15', level: 'INFO', message: 'Building production bundle...' },
  { timestamp: '10:42:35', level: 'ERROR', message: 'Connection timeout: Failed to reach database at 192.168.1.55:5432' },
  { timestamp: '10:42:36', level: 'INFO', message: 'Retrying connection (Attempt 1/3)...' },
  { timestamp: '10:42:38', level: 'INFO', message: 'Connection established successfully.' },
  { timestamp: '10:42:40', level: 'INFO', message: 'Running unit tests (Jest)...' },
  { timestamp: '10:42:42', level: 'INFO', message: "Test Suite 'Auth':\n  ✓ User can login (45ms)\n  ✓ User can logout (30ms)\n  ✓ Password reset email sent (120ms)" },
  { timestamp: '10:43:01', level: 'INFO', message: 'Uploading assets to S3 bucket...' },
]

const filterChips = [
  { id: 'all', label: 'All Logs' },
  { id: 'info', label: 'INFO', color: '#60a5fa' },
  { id: 'warn', label: 'WARN', color: '#facc15' },
  { id: 'error', label: 'ERROR', color: '#f87171' },
]

export function LogStreamPage() {
  const { jobId } = useParams()
  const navigate = useNavigate()
  const [search, setSearch] = useState('')
  const [activeFilter, setActiveFilter] = useState('all')
  const [isPaused, setIsPaused] = useState(false)

  const filteredLogs = mockLogs.filter((log) => {
    if (activeFilter !== 'all' && log.level.toLowerCase() !== activeFilter) {
      return false
    }
    if (search && !log.message.toLowerCase().includes(search.toLowerCase())) {
      return false
    }
    return true
  })

  return (
    <div className="flex flex-col h-screen overflow-hidden">
      {/* Top Navigation Bar */}
      <header className="bg-white dark:bg-background-dark border-b border-gray-200 dark:border-gray-800 shrink-0 z-20">
        <div className="flex items-center justify-between px-4 py-3">
          <button
            onClick={() => navigate(-1)}
            className="flex items-center justify-center text-gray-500 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-full p-2 -ml-2 transition-colors"
          >
            <span className="material-symbols-outlined">arrow_back_ios_new</span>
          </button>
          <div className="flex flex-col items-center">
            <h1 className="text-gray-900 dark:text-white text-base font-bold leading-tight tracking-tight">
              Build #{jobId || '4022'}
            </h1>
            <span className="text-xs text-gray-500 dark:text-gray-400 font-medium">Deploy to Prod</span>
          </div>
          <div className="flex items-center gap-2">
            <span className="relative flex h-2.5 w-2.5">
              <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-green-400 opacity-75"></span>
              <span className="relative inline-flex rounded-full h-2.5 w-2.5 bg-green-500"></span>
            </span>
            <span className="text-green-600 dark:text-green-500 text-xs font-bold uppercase tracking-wider">Run</span>
          </div>
        </div>
      </header>

      {/* Controls & Meta Area */}
      <div className="bg-white dark:bg-background-dark border-b border-gray-200 dark:border-gray-800 shrink-0 shadow-sm z-10">
        {/* Job Meta Info */}
        <div className="flex justify-between items-center px-4 py-2 bg-gray-50 dark:bg-[#161d2b]">
          <p className="text-gray-500 dark:text-[#92a4c9] text-xs font-medium">
            Job ID: <span className="text-gray-900 dark:text-white">#{jobId || '8821'}</span>
          </p>
          <p className="text-gray-500 dark:text-[#92a4c9] text-xs font-medium">
            Duration: <span className="text-gray-900 dark:text-white font-mono">04m 12s</span>
          </p>
        </div>

        {/* Search Bar */}
        <div className="px-4 py-3">
          <div className="relative group">
            <div className="absolute inset-y-0 left-0 pl-3 flex items-center pointer-events-none">
              <span className="material-symbols-outlined text-gray-400 group-focus-within:text-primary transition-colors">
                search
              </span>
            </div>
            <input
              type="text"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              className="block w-full pl-10 pr-3 py-2.5 border-none rounded-lg leading-5 bg-gray-100 dark:bg-[#232f48] text-gray-900 dark:text-white placeholder-gray-500 dark:placeholder-gray-400 focus:outline-none focus:ring-2 focus:ring-primary/50 transition-all text-sm"
              placeholder="Search logs (grep)..."
            />
          </div>
        </div>

        {/* Filter Chips */}
        <div className="flex gap-2 px-4 pb-3 overflow-x-auto scrollbar-hide">
          {filterChips.map((chip) => (
            <button
              key={chip.id}
              onClick={() => setActiveFilter(chip.id)}
              className={`flex items-center justify-center px-4 py-1.5 rounded-full text-xs font-semibold whitespace-nowrap transition-transform active:scale-95 ${
                activeFilter === chip.id
                  ? 'bg-primary text-white shadow-sm hover:shadow'
                  : 'bg-gray-100 dark:bg-[#232f48] text-gray-600 dark:text-gray-300 border border-transparent dark:border-gray-700 hover:bg-gray-200 dark:hover:bg-[#2f3e5e]'
              }`}
            >
              {chip.color && <span className="w-2 h-2 rounded-full mr-2" style={{ backgroundColor: chip.color }} />}
              {chip.label}
            </button>
          ))}
        </div>
      </div>

      {/* Log Stream Console */}
      <div
        className="flex-1 overflow-y-auto bg-gray-900 dark:bg-console-bg p-4 log-scroll relative font-mono text-sm leading-6"
        id="log-container"
      >
        {filteredLogs.map((log, index) => (
          <LogLine key={index} {...log} />
        ))}
        <div className="h-10"></div>
      </div>

      {/* Floating Scroll Button */}
      <div className="absolute bottom-20 right-4 z-30">
        <button
          aria-label="Scroll to bottom"
          className="flex items-center justify-center w-10 h-10 rounded-full bg-primary/90 text-white shadow-lg backdrop-blur-sm border border-white/10 hover:bg-primary transition-all active:scale-95"
        >
          <span className="material-symbols-outlined text-xl">arrow_downward</span>
        </button>
      </div>

      {/* Sticky Action Bar */}
      <div className="bg-white dark:bg-background-dark border-t border-gray-200 dark:border-gray-800 shrink-0 p-4 pb-6 z-20">
        <div className="flex items-center gap-3">
          <button
            onClick={() => setIsPaused(!isPaused)}
            className="flex-1 flex items-center justify-center gap-2 h-12 rounded-xl bg-gray-100 dark:bg-[#232f48] text-gray-900 dark:text-white font-medium hover:bg-gray-200 dark:hover:bg-[#2f3e5e] transition-colors active:scale-95"
          >
            <span className="material-symbols-outlined filled">{isPaused ? 'play_circle' : 'pause_circle'}</span>
            <span>{isPaused ? 'Resume' : 'Pause'}</span>
          </button>
          <button
            aria-label="Clear logs"
            className="flex items-center justify-center w-12 h-12 rounded-xl bg-gray-100 dark:bg-[#232f48] text-gray-500 dark:text-[#92a4c9] hover:text-red-500 dark:hover:text-red-400 hover:bg-gray-200 dark:hover:bg-[#2f3e5e] transition-colors active:scale-95"
          >
            <span className="material-symbols-outlined">delete_sweep</span>
          </button>
          <button
            aria-label="More options"
            className="flex items-center justify-center w-12 h-12 rounded-xl bg-gray-100 dark:bg-[#232f48] text-gray-500 dark:text-[#92a4c9] hover:bg-gray-200 dark:hover:bg-[#2f3e5e] transition-colors active:scale-95"
          >
            <span className="material-symbols-outlined">tune</span>
          </button>
        </div>
      </div>
    </div>
  )
}

function LogLine({ timestamp, level, message }: LogEntry) {
  const levelConfig = {
    INFO: { color: 'text-blue-400', bgHover: 'hover:bg-white/5', border: '' },
    WARN: { color: 'text-yellow-400', bgHover: 'hover:bg-yellow-500/10', border: 'border-l-2 border-yellow-500/50 bg-yellow-500/5' },
    ERROR: { color: 'text-red-400', bgHover: 'hover:bg-red-500/10', border: 'border-l-2 border-red-500/50 bg-red-500/5' },
  }

  const config = levelConfig[level]
  const messageColor = level === 'WARN' ? 'text-yellow-100' : level === 'ERROR' ? 'text-red-100' : 'text-gray-300'

  return (
    <div className={`flex gap-3 mb-1 group ${config.bgHover} -mx-4 px-4 py-0.5 transition-colors ${config.border}`}>
      <div className="text-gray-500 select-none text-[11px] pt-[2px] w-[54px] shrink-0 text-right opacity-60">
        {timestamp}
      </div>
      <div className="flex-1 break-words">
        <span className={`${config.color} font-bold text-[11px] mr-2 tracking-wide`}>[{level}]</span>
        <span className={messageColor} dangerouslySetInnerHTML={{ __html: message.replace(/\n/g, '<br/>') }} />
      </div>
    </div>
  )
}
