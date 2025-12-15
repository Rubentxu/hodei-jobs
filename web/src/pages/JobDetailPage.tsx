import { useState } from 'react'
import { Link, useParams, useNavigate } from 'react-router-dom'
import { useJob, useCancelJob } from '@/hooks/useJobs'

type TabId = 'overview' | 'config' | 'logs' | 'resources'

interface TimelineStep {
  label: string
  time: string
  detail?: string
  status: 'completed' | 'current' | 'pending'
}

export function JobDetailPage() {
  const { jobId } = useParams<{ jobId: string }>()
  const navigate = useNavigate()
  const { data: job, isLoading } = useJob(jobId || '')
  const cancelJob = useCancelJob()
  const [activeTab, setActiveTab] = useState<TabId>('overview')

  const tabs: { id: TabId; label: string }[] = [
    { id: 'overview', label: 'Overview' },
    { id: 'config', label: 'Config' },
    { id: 'logs', label: 'Logs' },
    { id: 'resources', label: 'Res' },
  ]

  const timelineSteps: TimelineStep[] = [
    { label: 'Job Queued', time: '10:42 AM', status: 'completed' },
    { label: 'Image Pulled', time: '10:43 AM', detail: 'python:3.9-slim', status: 'completed' },
    { label: 'Running Script', time: 'Now', detail: 'Executing main entrypoint...', status: 'current' },
    { label: 'Cleanup', time: '--:--', status: 'pending' },
  ]

  const handleCancel = async () => {
    if (!jobId) return
    if (window.confirm('Are you sure you want to cancel this job?')) {
      await cancelJob.mutateAsync(jobId)
      navigate('/jobs')
    }
  }

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-screen">
        <span className="material-symbols-outlined animate-spin text-primary text-4xl">sync</span>
      </div>
    )
  }

  return (
    <div className="bg-background-light dark:bg-background-dark text-slate-900 dark:text-white h-screen flex flex-col overflow-hidden">
      {/* Top App Bar */}
      <header className="flex items-center justify-between p-4 bg-background-light dark:bg-background-dark border-b border-slate-200 dark:border-slate-800 shrink-0 z-20">
        <button
          onClick={() => navigate(-1)}
          className="flex items-center justify-center w-10 h-10 rounded-full hover:bg-slate-200 dark:hover:bg-surface-highlight transition-colors text-slate-600 dark:text-slate-300"
        >
          <span className="material-symbols-outlined">arrow_back</span>
        </button>
        <h1 className="text-base font-bold tracking-tight">Job Details</h1>
        <button className="flex items-center justify-center w-10 h-10 rounded-full hover:bg-slate-200 dark:hover:bg-surface-highlight transition-colors text-slate-600 dark:text-slate-300">
          <span className="material-symbols-outlined">more_vert</span>
        </button>
      </header>

      {/* Main Content Area */}
      <main className="flex-1 overflow-y-auto overflow-x-hidden relative w-full">
        {/* Job Header Section */}
        <div className="px-5 pt-6 pb-2">
          <div className="flex items-start justify-between mb-2">
            <h2 className="text-3xl font-bold tracking-tight text-slate-900 dark:text-white">
              #JOB-{jobId}
            </h2>
            <div className="flex items-center gap-2 bg-primary/20 text-primary px-3 py-1 rounded-full">
              <div className="w-2 h-2 rounded-full bg-primary pulsing-dot"></div>
              <span className="text-xs font-bold uppercase tracking-wide">
                {job?.status || 'Running'}
              </span>
            </div>
          </div>

          {/* Metadata Chips */}
          <div className="flex gap-2 flex-wrap mt-3">
            <div className="flex items-center gap-1.5 px-2.5 py-1 rounded-md bg-slate-200 dark:bg-surface-highlight border border-transparent dark:border-slate-700">
              <span className="material-symbols-outlined text-[16px] text-slate-500 dark:text-slate-400">account_tree</span>
              <span className="text-xs font-medium text-slate-700 dark:text-slate-300">DataPipeline</span>
            </div>
            <div className="flex items-center gap-1.5 px-2.5 py-1 rounded-md bg-slate-200 dark:bg-surface-highlight border border-transparent dark:border-slate-700">
              <span className="material-symbols-outlined text-[16px] text-slate-500 dark:text-slate-400">timer</span>
              <span className="text-xs font-medium text-slate-700 dark:text-slate-300">{job?.duration || '4m 12s'}</span>
            </div>
            <div className="flex items-center gap-1.5 px-2.5 py-1 rounded-md bg-slate-200 dark:bg-surface-highlight border border-transparent dark:border-slate-700">
              <span className="material-symbols-outlined text-[16px] text-slate-500 dark:text-slate-400">person</span>
              <span className="text-xs font-medium text-slate-700 dark:text-slate-300">admin</span>
            </div>
          </div>
        </div>

        {/* Segmented Control (Tabs) */}
        <div className="px-5 py-4 sticky top-0 bg-background-light dark:bg-background-dark z-10">
          <div className="flex p-1 bg-slate-200 dark:bg-surface-dark rounded-lg">
            {tabs.map((tab) => (
              <label key={tab.id} className="flex-1 cursor-pointer">
                <input
                  type="radio"
                  name="tabs"
                  className="peer sr-only"
                  checked={activeTab === tab.id}
                  onChange={() => setActiveTab(tab.id)}
                />
                <div className="flex items-center justify-center py-2 text-sm font-medium rounded-md text-slate-500 dark:text-slate-400 peer-checked:bg-white dark:peer-checked:bg-primary peer-checked:text-primary dark:peer-checked:text-white peer-checked:shadow-sm transition-all">
                  {tab.label}
                </div>
              </label>
            ))}
          </div>
        </div>

        {/* Tab Content: Overview */}
        {activeTab === 'overview' && (
          <div className="px-5 pb-24 space-y-6">
            {/* Execution Timeline */}
            <section>
              <h3 className="text-sm font-bold uppercase tracking-wider text-slate-500 dark:text-slate-400 mb-3">Timeline</h3>
              <div className="bg-white dark:bg-surface-dark rounded-xl p-5 shadow-sm border border-slate-200 dark:border-slate-800">
                <div className="relative pl-4 border-l-2 border-slate-200 dark:border-slate-700 space-y-6">
                  {timelineSteps.map((step, index) => (
                    <div key={index} className={`relative ${step.status === 'pending' ? 'opacity-50' : ''}`}>
                      <div
                        className={`absolute -left-[23px] h-4 w-4 rounded-full border-4 border-white dark:border-surface-dark box-content ${
                          step.status === 'completed'
                            ? 'bg-green-500'
                            : step.status === 'current'
                            ? 'bg-primary shadow-[0_0_0_4px_rgba(43,108,238,0.2)]'
                            : 'bg-slate-300 dark:bg-slate-600'
                        }`}
                      ></div>
                      <div className="flex justify-between items-center">
                        <span
                          className={`text-sm font-medium ${
                            step.status === 'current'
                              ? 'font-bold text-primary'
                              : 'text-slate-900 dark:text-white'
                          }`}
                        >
                          {step.label}
                        </span>
                        <span
                          className={`text-xs ${
                            step.status === 'current' ? 'text-primary font-medium' : 'text-slate-500'
                          }`}
                        >
                          {step.time}
                        </span>
                      </div>
                      {step.detail && (
                        <p className={`text-xs text-slate-500 mt-1 ${step.label === 'Image Pulled' ? 'font-mono' : ''}`}>
                          {step.detail}
                        </p>
                      )}
                    </div>
                  ))}
                </div>
              </div>
            </section>

            {/* Resource Snapshot */}
            <section>
              <div className="flex justify-between items-end mb-3">
                <h3 className="text-sm font-bold uppercase tracking-wider text-slate-500 dark:text-slate-400">Live Resources</h3>
                <span className="text-xs text-primary cursor-pointer hover:underline">View History</span>
              </div>
              <div className="grid grid-cols-2 gap-3">
                {/* CPU Card */}
                <div className="bg-white dark:bg-surface-dark rounded-xl p-4 shadow-sm border border-slate-200 dark:border-slate-800">
                  <div className="flex items-center gap-2 mb-2">
                    <span className="material-symbols-outlined text-purple-500 text-lg">memory</span>
                    <span className="text-xs font-medium text-slate-500 dark:text-slate-400">CPU Usage</span>
                  </div>
                  <div className="flex items-end gap-2">
                    <span className="text-2xl font-bold text-slate-900 dark:text-white">{job?.cpuUsage || 42}%</span>
                    <span className="text-xs text-slate-500 mb-1">/ 2 Cores</span>
                  </div>
                  <div className="w-full h-1.5 bg-slate-100 dark:bg-slate-700 rounded-full mt-3 overflow-hidden">
                    <div className="h-full bg-purple-500 rounded-full" style={{ width: `${job?.cpuUsage || 42}%` }}></div>
                  </div>
                </div>

                {/* RAM Card */}
                <div className="bg-white dark:bg-surface-dark rounded-xl p-4 shadow-sm border border-slate-200 dark:border-slate-800">
                  <div className="flex items-center gap-2 mb-2">
                    <span className="material-symbols-outlined text-orange-500 text-lg">database</span>
                    <span className="text-xs font-medium text-slate-500 dark:text-slate-400">Memory</span>
                  </div>
                  <div className="flex items-end gap-2">
                    <span className="text-2xl font-bold text-slate-900 dark:text-white">
                      1.2<span className="text-base font-normal text-slate-500">GB</span>
                    </span>
                  </div>
                  <div className="w-full h-1.5 bg-slate-100 dark:bg-slate-700 rounded-full mt-3 overflow-hidden">
                    <div className="h-full bg-orange-500 rounded-full" style={{ width: `${job?.memoryUsage || 65}%` }}></div>
                  </div>
                </div>
              </div>
            </section>

            {/* Quick Logs Preview */}
            <section>
              <div className="flex justify-between items-center mb-3">
                <h3 className="text-sm font-bold uppercase tracking-wider text-slate-500 dark:text-slate-400">Latest Logs</h3>
                <span className="material-symbols-outlined text-slate-500 text-sm animate-spin">sync</span>
              </div>
              <div className="bg-[#0d1117] rounded-xl p-4 font-mono text-xs overflow-hidden border border-slate-800 shadow-inner">
                <div className="space-y-1 text-slate-400">
                  <div className="flex gap-2">
                    <span className="text-slate-600 select-none">104</span>
                    <span>[INFO] Connection established to db_primary</span>
                  </div>
                  <div className="flex gap-2">
                    <span className="text-slate-600 select-none">105</span>
                    <span>[INFO] Processing batch #{jobId}...</span>
                  </div>
                  <div className="flex gap-2">
                    <span className="text-slate-600 select-none">106</span>
                    <span className="text-yellow-400">[WARN] High latency detected on node-04</span>
                  </div>
                  <div className="flex gap-2">
                    <span className="text-slate-600 select-none">107</span>
                    <span className="text-white">_</span>
                  </div>
                </div>
              </div>
            </section>
          </div>
        )}

        {/* Tab Content: Config */}
        {activeTab === 'config' && (
          <div className="px-5 pb-24 space-y-4">
            <div className="bg-white dark:bg-surface-dark rounded-xl p-4 shadow-sm border border-slate-200 dark:border-slate-800">
              <h3 className="text-sm font-bold text-slate-500 dark:text-slate-400 mb-3">Configuration</h3>
              <div className="space-y-3 text-sm">
                <div className="flex justify-between">
                  <span className="text-slate-500">Command</span>
                  <span className="font-mono text-slate-900 dark:text-white">python main.py</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-slate-500">Image</span>
                  <span className="font-mono text-slate-900 dark:text-white">python:3.9-slim</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-slate-500">CPU Limit</span>
                  <span className="text-slate-900 dark:text-white">2 cores</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-slate-500">Memory Limit</span>
                  <span className="text-slate-900 dark:text-white">2 GB</span>
                </div>
              </div>
            </div>
          </div>
        )}

        {/* Tab Content: Logs */}
        {activeTab === 'logs' && (
          <div className="px-5 pb-24">
            <Link
              to={`/jobs/${jobId}/logs`}
              className="flex items-center justify-center gap-2 py-3 bg-primary text-white rounded-lg font-medium"
            >
              <span className="material-symbols-outlined">terminal</span>
              Open Full Log Viewer
            </Link>
          </div>
        )}

        {/* Tab Content: Resources */}
        {activeTab === 'resources' && (
          <div className="px-5 pb-24 space-y-4">
            <div className="bg-white dark:bg-surface-dark rounded-xl p-4 shadow-sm border border-slate-200 dark:border-slate-800">
              <h3 className="text-sm font-bold text-slate-500 dark:text-slate-400 mb-3">Resource Usage</h3>
              <p className="text-sm text-slate-500">Detailed resource graphs coming soon...</p>
            </div>
          </div>
        )}
      </main>

      {/* Bottom Action Bar */}
      <footer className="shrink-0 p-4 pb-6 bg-white dark:bg-background-dark border-t border-slate-200 dark:border-slate-800 z-20">
        <div className="flex gap-3">
          <button className="flex-1 h-12 flex items-center justify-center gap-2 rounded-lg border border-slate-300 dark:border-slate-600 bg-transparent text-slate-700 dark:text-white font-semibold text-sm hover:bg-slate-50 dark:hover:bg-surface-highlight transition-colors">
            <span className="material-symbols-outlined text-[20px]">terminal</span>
            SSH Access
          </button>
          <button
            onClick={handleCancel}
            disabled={cancelJob.isPending}
            className="flex-1 h-12 flex items-center justify-center gap-2 rounded-lg bg-red-500/10 text-red-600 dark:text-red-400 border border-transparent font-bold text-sm hover:bg-red-500/20 transition-colors disabled:opacity-50"
          >
            <span className="material-symbols-outlined text-[20px]">
              {cancelJob.isPending ? 'sync' : 'cancel'}
            </span>
            {cancelJob.isPending ? 'Cancelling...' : 'Cancel Job'}
          </button>
        </div>
      </footer>

      <style>{`
        .pulsing-dot {
          animation: pulse-ring 2s cubic-bezier(0.215, 0.61, 0.355, 1) infinite;
        }
        
        @keyframes pulse-ring {
          0% { transform: scale(0.8); box-shadow: 0 0 0 0 rgba(43, 108, 238, 0.7); }
          70% { transform: scale(1); box-shadow: 0 0 0 6px rgba(43, 108, 238, 0); }
          100% { transform: scale(0.8); box-shadow: 0 0 0 0 rgba(43, 108, 238, 0); }
        }
      `}</style>
    </div>
  )
}
