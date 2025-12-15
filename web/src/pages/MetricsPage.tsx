import { useState } from 'react'
import { useAggregatedMetrics } from '@/hooks/useMetrics'
import { useProviders } from '@/hooks/useProviders'

type TimeRange = '1H' | '24H' | '7D' | '30D'

export function MetricsPage() {
  const [timeRange, setTimeRange] = useState<TimeRange>('24H')
  const { data: metrics, isLoading } = useAggregatedMetrics(timeRange)
  const { data: providers } = useProviders()

  const timeRanges: TimeRange[] = ['1H', '24H', '7D', '30D']

  const barHeights = [40, 65, 45, 80, 60, 50, 90, 75, 55, 40, 30, 60]

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-screen bg-background-dark">
        <span className="material-symbols-outlined animate-spin text-primary text-4xl">sync</span>
      </div>
    )
  }

  return (
    <div className="bg-background-light dark:bg-background-dark text-slate-900 dark:text-white font-display antialiased selection:bg-primary selection:text-white overflow-x-hidden min-h-screen">
      {/* Sticky Top Bar */}
      <header className="sticky top-0 z-50 bg-background-dark/95 backdrop-blur-md border-b border-white/5">
        <div className="flex items-center p-4 justify-between max-w-md mx-auto w-full">
          <button className="text-white/70 hover:text-white transition-colors p-1 -ml-1 rounded-full active:bg-white/10">
            <span className="material-symbols-outlined text-[24px]">menu</span>
          </button>
          <h2 className="text-white text-lg font-bold leading-tight tracking-[-0.015em]">System Overview</h2>
          <button className="flex items-center justify-center rounded-full size-8 overflow-hidden border border-white/20">
            <span className="material-symbols-outlined text-[20px] text-white">person</span>
          </button>
        </div>
      </header>

      {/* Main Content */}
      <main className="flex flex-col w-full max-w-md mx-auto min-h-screen pb-8">
        {/* Time Range Selector */}
        <div className="px-4 py-4 w-full">
          <div className="flex h-10 w-full items-center justify-between rounded-lg bg-surface-dark p-1">
            {timeRanges.map((range) => (
              <label
                key={range}
                className={`flex cursor-pointer h-full flex-1 items-center justify-center rounded-md px-2 transition-all text-xs font-medium ${
                  timeRange === range
                    ? 'bg-primary text-white'
                    : 'text-slate-400 hover:text-white'
                }`}
              >
                <span>{range}</span>
                <input
                  type="radio"
                  name="time-range"
                  value={range}
                  checked={timeRange === range}
                  onChange={() => setTimeRange(range)}
                  className="hidden"
                />
              </label>
            ))}
          </div>
        </div>

        {/* KPI Grid */}
        <div className="grid grid-cols-2 gap-3 px-4 mb-6">
          {/* Card 1 - Total Jobs */}
          <div className="flex flex-col gap-1 rounded-xl p-4 bg-surface-dark border border-white/5">
            <div className="flex items-center gap-2 mb-1">
              <span className="material-symbols-outlined text-[18px] text-primary">analytics</span>
              <p className="text-slate-400 text-xs font-medium uppercase tracking-wider">Total Jobs</p>
            </div>
            <p className="text-white text-2xl font-bold tabular-nums">
              {metrics?.totalJobs.toLocaleString() || '14,205'}
            </p>
            <div className="flex items-center gap-1">
              <span className="material-symbols-outlined text-[14px] text-success">trending_up</span>
              <p className="text-success text-xs font-medium">+12%</p>
            </div>
          </div>

          {/* Card 2 - Success Rate */}
          <div className="flex flex-col gap-1 rounded-xl p-4 bg-surface-dark border border-white/5 relative overflow-hidden">
            <div className="absolute top-0 right-0 p-3 opacity-10">
              <span className="material-symbols-outlined text-[48px] text-success">check_circle</span>
            </div>
            <div className="flex items-center gap-2 mb-1 relative z-10">
              <span className="material-symbols-outlined text-[18px] text-success">check_circle</span>
              <p className="text-slate-400 text-xs font-medium uppercase tracking-wider">Success</p>
            </div>
            <p className="text-white text-2xl font-bold tabular-nums relative z-10">
              {metrics?.successRate.toFixed(1) || '98.2'}%
            </p>
            <div className="flex items-center gap-1 relative z-10">
              <span className="material-symbols-outlined text-[14px] text-success">trending_up</span>
              <p className="text-success text-xs font-medium">+0.5%</p>
            </div>
          </div>

          {/* Card 3 - Avg Duration */}
          <div className="flex flex-col gap-1 rounded-xl p-4 bg-surface-dark border border-white/5">
            <div className="flex items-center gap-2 mb-1">
              <span className="material-symbols-outlined text-[18px] text-warning">timer</span>
              <p className="text-slate-400 text-xs font-medium uppercase tracking-wider">Avg Duration</p>
            </div>
            <p className="text-white text-2xl font-bold tabular-nums">
              {Math.floor((metrics?.avgDuration || 252) / 60)}m {(metrics?.avgDuration || 252) % 60}s
            </p>
            <div className="flex items-center gap-1">
              <span className="material-symbols-outlined text-[14px] text-success">arrow_downward</span>
              <p className="text-success text-xs font-medium">-12s</p>
            </div>
          </div>

          {/* Card 4 - CPU Load */}
          <div className="flex flex-col gap-1 rounded-xl p-4 bg-surface-dark border border-white/5">
            <div className="flex items-center gap-2 mb-1">
              <span className="material-symbols-outlined text-[18px] text-primary">memory</span>
              <p className="text-slate-400 text-xs font-medium uppercase tracking-wider">CPU Load</p>
            </div>
            <p className="text-white text-2xl font-bold tabular-nums">{metrics?.cpuLoad || 78}%</p>
            <div className="flex items-center gap-1">
              <span className="material-symbols-outlined text-[14px] text-warning">trending_up</span>
              <p className="text-warning text-xs font-medium">+4%</p>
            </div>
          </div>
        </div>

        {/* Job Status Visualization */}
        <div className="px-4 mb-6">
          <h3 className="text-white text-base font-bold mb-3 flex items-center gap-2">
            Job Distribution
          </h3>
          <div className="bg-surface-dark rounded-xl p-5 border border-white/5">
            {/* Legend */}
            <div className="flex justify-between items-end mb-4">
              <div className="flex flex-col">
                <span className="text-xs text-slate-400 mb-1">Total Executions</span>
                <span className="text-xl font-bold text-white">
                  {(metrics?.jobDistribution.succeeded || 0) +
                    (metrics?.jobDistribution.failed || 0) +
                    (metrics?.jobDistribution.cancelled || 0) || '1,240'}
                </span>
              </div>
              <div className="flex gap-3">
                <div className="flex items-center gap-1.5">
                  <div className="w-2 h-2 rounded-full bg-success"></div>
                  <span className="text-xs text-slate-300">Success</span>
                </div>
                <div className="flex items-center gap-1.5">
                  <div className="w-2 h-2 rounded-full bg-danger"></div>
                  <span className="text-xs text-slate-300">Fail</span>
                </div>
                <div className="flex items-center gap-1.5">
                  <div className="w-2 h-2 rounded-full bg-slate-500"></div>
                  <span className="text-xs text-slate-300">Pending</span>
                </div>
              </div>
            </div>

            {/* Stacked Bar */}
            <div className="flex h-6 w-full rounded-full overflow-hidden bg-surface-darker">
              <div className="bg-success h-full" style={{ width: '72%' }}></div>
              <div className="bg-danger h-full border-l border-surface-dark" style={{ width: '18%' }}></div>
              <div className="bg-slate-500 h-full border-l border-surface-dark" style={{ width: '10%' }}></div>
            </div>
            <div className="flex justify-between mt-2 text-[10px] text-slate-500 font-medium">
              <span>0%</span>
              <span>25%</span>
              <span>50%</span>
              <span>75%</span>
              <span>100%</span>
            </div>
          </div>
        </div>

        {/* Charts: Volume Trend */}
        <div className="px-4 mb-6">
          <div className="flex justify-between items-center mb-3">
            <h3 className="text-white text-base font-bold">Execution Trends</h3>
            <span className="text-xs text-primary font-medium bg-primary/10 px-2 py-1 rounded">
              Last {timeRange}
            </span>
          </div>
          <div className="bg-surface-dark rounded-xl p-5 border border-white/5">
            <div className="flex h-[140px] items-end justify-between gap-1 sm:gap-2">
              {barHeights.map((height, index) => (
                <div
                  key={index}
                  className={`w-full rounded-t-sm transition-colors ${
                    index === 6
                      ? 'bg-primary hover:bg-primary-light shadow-[0_0_10px_rgba(43,108,238,0.3)]'
                      : 'bg-primary/20 hover:bg-primary/40'
                  }`}
                  style={{ height: `${height}%` }}
                ></div>
              ))}
            </div>
            {/* X-Axis Labels */}
            <div className="flex justify-between mt-3 text-[10px] text-slate-500 font-medium uppercase">
              <span>00:00</span>
              <span>06:00</span>
              <span>12:00</span>
              <span>18:00</span>
            </div>
          </div>
        </div>

        {/* Provider Health List */}
        <div className="px-4 pb-8">
          <h3 className="text-white text-base font-bold mb-3">Active Providers</h3>
          <div className="flex flex-col gap-3">
            {providers?.slice(0, 3).map((provider) => (
              <div
                key={provider.id}
                className={`bg-surface-dark border border-white/5 rounded-xl p-4 ${
                  provider.status === 'OFFLINE' ? 'opacity-75' : ''
                }`}
              >
                <div className="flex justify-between items-start mb-3">
                  <div className="flex items-center gap-3">
                    <div className="bg-surface-darker p-2 rounded-lg">
                      <span
                        className={`material-symbols-outlined text-[20px] ${
                          provider.status === 'OFFLINE' ? 'text-slate-400' : 'text-white'
                        }`}
                      >
                        dns
                      </span>
                    </div>
                    <div>
                      <h4
                        className={`text-sm font-semibold ${
                          provider.status === 'OFFLINE' ? 'text-slate-300' : 'text-white'
                        }`}
                      >
                        {provider.name}
                      </h4>
                      <p className="text-slate-500 text-xs">Provider #{provider.id.slice(-2)}</p>
                    </div>
                  </div>
                  <span
                    className={`inline-flex items-center rounded-md px-2 py-1 text-xs font-medium ring-1 ring-inset ${
                      provider.status === 'ACTIVE'
                        ? 'bg-success/10 text-success ring-success/20'
                        : provider.status === 'OVERLOADED'
                        ? 'bg-warning/10 text-warning ring-warning/20'
                        : 'bg-slate-400/10 text-slate-400 ring-slate-400/20'
                    }`}
                  >
                    {provider.status === 'ACTIVE'
                      ? 'Active'
                      : provider.status === 'OVERLOADED'
                      ? 'High Load'
                      : 'Idle'}
                  </span>
                </div>
                <div className="grid grid-cols-2 gap-4">
                  <div className="flex flex-col gap-1">
                    <div className="flex justify-between text-xs mb-1">
                      <span className={provider.status === 'OFFLINE' ? 'text-slate-500' : 'text-slate-400'}>
                        CPU
                      </span>
                      <span
                        className={`font-mono ${
                          provider.status === 'OVERLOADED'
                            ? 'text-warning'
                            : provider.status === 'OFFLINE'
                            ? 'text-slate-400'
                            : 'text-white'
                        }`}
                      >
                        {provider.cpuUsage}%
                      </span>
                    </div>
                    <div className="w-full bg-surface-darker rounded-full h-1.5">
                      <div
                        className={`h-1.5 rounded-full ${
                          provider.status === 'OVERLOADED'
                            ? 'bg-warning'
                            : provider.status === 'OFFLINE'
                            ? 'bg-slate-500'
                            : 'bg-primary'
                        }`}
                        style={{ width: `${provider.cpuUsage}%` }}
                      ></div>
                    </div>
                  </div>
                  <div className="flex flex-col gap-1">
                    <div className="flex justify-between text-xs mb-1">
                      <span className={provider.status === 'OFFLINE' ? 'text-slate-500' : 'text-slate-400'}>
                        Memory
                      </span>
                      <span
                        className={`font-mono ${
                          provider.status === 'OFFLINE' ? 'text-slate-400' : 'text-white'
                        }`}
                      >
                        {((provider.memoryUsage || 0) * 4 / 100).toFixed(1)} GB
                      </span>
                    </div>
                    <div className="w-full bg-surface-darker rounded-full h-1.5">
                      <div
                        className={`h-1.5 rounded-full ${
                          provider.status === 'OFFLINE' ? 'bg-slate-500' : 'bg-purple-500'
                        }`}
                        style={{ width: `${provider.memoryUsage}%` }}
                      ></div>
                    </div>
                  </div>
                </div>
              </div>
            ))}
          </div>
        </div>
      </main>
    </div>
  )
}
