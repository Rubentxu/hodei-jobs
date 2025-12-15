import { useState } from 'react'
import { Header } from '@/components/layout/Header'
import { SearchInput } from '@/components/ui/SearchInput'
import { FilterChips, FilterChip } from '@/components/ui/FilterChips'
import { JobCard, JobStatus } from '@/components/jobs/JobCard'

const filterChips: FilterChip[] = [
  { id: 'all', label: 'All Jobs' },
  { id: 'running', label: 'Running', color: '#2b6cee' },
  { id: 'failed', label: 'Failed', color: '#ef4444' },
  { id: 'succeeded', label: 'Succeeded', color: '#10b981' },
  { id: 'pending', label: 'Pending', color: '#94a3b8' },
]

interface Job {
  id: string
  name: string
  status: JobStatus
  startedAt: string
  duration: string
  progress?: number
  date: 'today' | 'yesterday' | 'older'
}

const mockJobs: Job[] = [
  { id: '5021', name: 'AWS_Production_Build', status: 'RUNNING', startedAt: '10:05 AM', duration: '2m 15s', progress: 45, date: 'today' },
  { id: '5020', name: 'Azure_West_Deploy', status: 'FAILED', startedAt: '09:45 AM', duration: '45s', date: 'today' },
  { id: '5019', name: 'GCP_Data_Sync', status: 'SUCCEEDED', startedAt: '09:15 AM', duration: '12m 30s', date: 'today' },
  { id: '5018', name: 'AWS_Production_Backup', status: 'SUCCEEDED', startedAt: '11:30 PM', duration: '45m 12s', date: 'yesterday' },
  { id: '5017', name: 'Legacy_Import_Job', status: 'PENDING', startedAt: '11:15 PM', duration: '-', date: 'yesterday' },
  { id: '5016', name: 'Frontend_Build_V2', status: 'SUCCEEDED', startedAt: '10:00 PM', duration: '5m 43s', date: 'yesterday' },
]

export function JobHistoryPage() {
  const [search, setSearch] = useState('')
  const [activeFilter, setActiveFilter] = useState('all')

  const filteredJobs = mockJobs.filter((job) => {
    if (activeFilter !== 'all' && job.status.toLowerCase() !== activeFilter) {
      return false
    }
    if (search && !job.id.includes(search) && !job.name.toLowerCase().includes(search.toLowerCase())) {
      return false
    }
    return true
  })

  const todayJobs = filteredJobs.filter((j) => j.date === 'today')
  const yesterdayJobs = filteredJobs.filter((j) => j.date === 'yesterday')

  return (
    <>
      <Header title="Job History" showFilter />
      <SearchInput
        placeholder="Search Job ID or Provider..."
        value={search}
        onChange={setSearch}
      />
      <FilterChips chips={filterChips} activeId={activeFilter} onChange={setActiveFilter} />

      <div className="flex flex-col gap-3 px-4 py-2">
        {todayJobs.length > 0 && (
          <>
            <div className="py-2">
              <h3 className="text-xs font-semibold text-gray-400 uppercase tracking-wider pl-1">
                Today
              </h3>
            </div>
            {todayJobs.map((job) => (
              <JobCard key={job.id} {...job} />
            ))}
          </>
        )}

        {yesterdayJobs.length > 0 && (
          <>
            <div className="py-2 mt-2">
              <h3 className="text-xs font-semibold text-gray-400 uppercase tracking-wider pl-1">
                Yesterday
              </h3>
            </div>
            {yesterdayJobs.map((job) => (
              <JobCard key={job.id} {...job} isOld />
            ))}
          </>
        )}

        <div className="py-4 text-center">
          <p className="text-xs text-gray-500 dark:text-gray-600">End of recent history</p>
        </div>
      </div>
    </>
  )
}
