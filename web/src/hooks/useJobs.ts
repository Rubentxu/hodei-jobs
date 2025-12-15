import {
  CancelJobRequest,
  GetJobRequest,
  GetJobResponse,
  JobDefinition,
  JobId,
  ListJobsRequest,
  ListJobsResponse,
  JobStatus as ProtoJobStatus,
  QueueJobRequest
} from '@/gen/hodei_all_in_one_pb';
import { jobClient } from '@/lib/connect';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

// Types
export type JobStatus = 'PENDING' | 'RUNNING' | 'SUCCEEDED' | 'FAILED' | 'CANCELLED' | 'TIMEOUT' | 'ASSIGNED'

function mapProtoStatus(status: ProtoJobStatus): JobStatus {
  switch (status) {
    case ProtoJobStatus.PENDING: return 'PENDING';
    case ProtoJobStatus.ASSIGNED: return 'ASSIGNED';
    case ProtoJobStatus.RUNNING: return 'RUNNING';
    case ProtoJobStatus.COMPLETED: return 'SUCCEEDED';
    case ProtoJobStatus.FAILED: return 'FAILED';
    case ProtoJobStatus.CANCELLED: return 'CANCELLED';
    case ProtoJobStatus.TIMEOUT: return 'TIMEOUT';
    default: return 'PENDING';
  }
}

export interface Job {
  id: string
  name: string
  status: JobStatus
  startedAt?: string
  startedAtRaw?: Date
  completedAt?: string
  completedAtRaw?: Date
  duration?: string
  progress?: number
  command?: string
  providerId?: string
  cpuUsage?: number
  memoryUsage?: number
}

export interface JobsFilter {
  status?: JobStatus
  search?: string
  limit?: number
  offset?: number
}

// Query keys
export const jobKeys = {
  all: ['jobs'] as const,
  lists: () => [...jobKeys.all, 'list'] as const,
  list: (filters: JobsFilter) => [...jobKeys.lists(), filters] as const,
  details: () => [...jobKeys.all, 'detail'] as const,
  detail: (id: string) => [...jobKeys.details(), id] as const,
}

// Mock data for development fallback
const mockJobs: Job[] = [
  { id: '5021', name: 'AWS_Production_Build', status: 'RUNNING', startedAt: '10:05 AM', duration: '2m 15s', progress: 45 },
  { id: '5020', name: 'Database_Migration', status: 'FAILED', startedAt: '09:45 AM', duration: '45s' },
  { id: '5019', name: 'Frontend_Deploy', status: 'SUCCEEDED', startedAt: '09:15 AM', duration: '12m 30s' },
  { id: '5018', name: 'Backup_Script', status: 'PENDING', startedAt: '11:15 PM', duration: '-' },
  { id: '5017', name: 'API_Tests', status: 'SUCCEEDED', startedAt: 'Yesterday', duration: '8m 12s' },
  { id: '5016', name: 'Cleanup_Task', status: 'FAILED', startedAt: 'Yesterday', duration: '2m 5s' },
]

// Helper to use mock data when backend is unavailable
const USE_MOCK = import.meta.env.VITE_USE_MOCK === 'true' || !import.meta.env.VITE_API_URL

// Hooks
export function useJobs(filters: JobsFilter = {}) {
  return useQuery({
    queryKey: jobKeys.list(filters),
    queryFn: async () => {
      if (USE_MOCK) {
        await new Promise(resolve => setTimeout(resolve, 300))
        let filtered = [...mockJobs]
        if (filters.status) {
          filtered = filtered.filter(job => job.status === filters.status)
        }
        if (filters.search) {
          const search = filters.search.toLowerCase()
          filtered = filtered.filter(job =>
            job.id.includes(search) ||
            job.name.toLowerCase().includes(search)
          )
        }
        return filtered
      }

      // Real gRPC call
      try {
        const request = new ListJobsRequest({
          limit: filters.limit || 10,
          offset: filters.offset || 0,
        })

        const response = await jobClient.listJobs(request) as ListJobsResponse

        return response.jobs.map(summary => {
          const startedDate = summary.startedAt ? new Date(Number(summary.startedAt.seconds) * 1000) : undefined
          const completedDate = summary.completedAt ? new Date(Number(summary.completedAt.seconds) * 1000) : undefined

          return {
            id: summary.jobId?.value || '',
            name: summary.name,
            status: mapProtoStatus(summary.status),
            startedAt: startedDate?.toLocaleTimeString(),
            startedAtRaw: startedDate,
            completedAt: completedDate?.toLocaleTimeString(),
            completedAtRaw: completedDate,
            duration: summary.duration ? `${summary.duration.seconds}s` : undefined,
            progress: summary.progressPercentage,
          }
        })
      } catch (err) {
        console.error('Failed to list jobs:', err)
        return []
      }
    },
  })
}

export function useJob(id: string) {
  return useQuery({
    queryKey: jobKeys.detail(id),
    queryFn: async () => {
      if (USE_MOCK) {
        await new Promise(resolve => setTimeout(resolve, 300))
        return mockJobs.find(job => job.id === id) || null
      }

      // Real gRPC call
      try {
        const request = new GetJobRequest({
          jobId: new JobId({ value: id })
        })

        const response = await jobClient.getJob(request) as GetJobResponse
        const jobDef = response.job

        if (!jobDef) return null

        return {
          id: jobDef.jobId?.value || id,
          name: jobDef.name,
          status: mapProtoStatus(response.status),
          command: jobDef.command || '',
          providerId: undefined, // Not directly in GetJobResponse unless we look at executions
          cpuUsage: 0, // Not in GetJobResponse
          memoryUsage: 0, // Not in GetJobResponse
          duration: undefined,
        }
      } catch (err) {
        console.error('Failed to get job:', err)
        return null
      }
    },
    enabled: !!id,
  })
}

export function useQueueJob() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: async (jobData: Partial<Job>) => {
      if (USE_MOCK) {
        await new Promise(resolve => setTimeout(resolve, 500))
        const newJob: Job = {
          id: String(Date.now()),
          name: jobData.name || 'New Job',
          status: 'PENDING',
          startedAt: new Date().toLocaleTimeString(),
          duration: '-',
          ...jobData,
        }
        return newJob
      }

      // Real gRPC call
      try {
        const jobDefinition = new JobDefinition({
          name: jobData.name || 'New Job',
          command: jobData.command || 'echo "Hello"',
          arguments: [],
          environment: {},
        })

        await jobClient.queueJob(new QueueJobRequest({
          jobDefinition,
          queuedBy: 'web-dashboard',
        }))

        return {
          id: String(Date.now()),
          name: jobData.name || 'New Job',
          status: 'PENDING' as JobStatus,
          startedAt: new Date().toLocaleTimeString(),
          duration: '-',
        }
      } catch (err) {
        console.error('Failed to queue job via gRPC:', err)
        throw err
      }
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: jobKeys.lists() })
    },
  })
}

export function useCancelJob() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: async (jobId: string) => {
      if (USE_MOCK) {
        await new Promise(resolve => setTimeout(resolve, 500))
        return { success: true, jobId }
      }

      // Real gRPC call
      try {
        await jobClient.cancelJob(new CancelJobRequest({
          jobId: new JobId({ value: jobId }),
          reason: 'Cancelled by user from web dashboard',
        }))
        return { success: true, jobId }
      } catch (err) {
        console.error('Failed to cancel job via gRPC:', err)
        throw err
      }
    },
    onSuccess: (_, jobId) => {
      queryClient.invalidateQueries({ queryKey: jobKeys.detail(jobId) })
      queryClient.invalidateQueries({ queryKey: jobKeys.lists() })
    },
  })
}
