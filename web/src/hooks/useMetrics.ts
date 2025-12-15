import {
  GetQueueStatusRequest,
  MetricsQuery,
  QueueStatus,
  type AggregatedMetrics,
  type GetQueueStatusResponse
} from '@/gen/hodei_all_in_one_pb'
import {
  ListProvidersRequest,
  type ListProvidersResponse
} from '@/gen/provider_management_pb'
import { metricsClient, providerClient, schedulerClient } from '@/lib/connect'
import { Timestamp } from '@bufbuild/protobuf'
import { useQuery } from '@tanstack/react-query'

// Types - will be replaced by generated types from proto
export interface SystemMetricsUI {
  totalJobs: number
  runningJobs: number
  failedJobs: number
  succeededJobs: number
  pendingJobs: number
  cpuLoad: number
  memoryUsage: number
  uptime: number
  activeNodes: number
  queueSize: number
}

export interface AggregatedMetricsUI {
  timeRange: '1H' | '24H' | '7D' | '30D'
  totalJobs: number
  successRate: number
  avgDuration: number
  cpuLoad: number
  jobDistribution: {
    succeeded: number
    failed: number
    cancelled: number
  }
  executionTrends: {
    label: string
    succeeded: number
    failed: number
  }[]
}

// Query keys
export const metricsKeys = {
  all: ['metrics'] as const,
  system: () => [...metricsKeys.all, 'system'] as const,
  aggregated: (timeRange: string) => [...metricsKeys.all, 'aggregated', timeRange] as const,
}

// Mock data as fallback
const mockSystemMetrics: SystemMetricsUI = {
  totalJobs: 1240,
  runningJobs: 12,
  failedJobs: 23,
  succeededJobs: 1205,
  pendingJobs: 0,
  cpuLoad: 42,
  memoryUsage: 68,
  uptime: 98,
  activeNodes: 2,
  queueSize: 3,
}

const mockAggregatedMetrics: Record<string, AggregatedMetricsUI> = {
  '1H': {
    timeRange: '1H',
    totalJobs: 45,
    successRate: 94.2,
    avgDuration: 180,
    cpuLoad: 42,
    jobDistribution: { succeeded: 42, failed: 2, cancelled: 1 },
    executionTrends: [
      { label: '12:00', succeeded: 8, failed: 0 },
      { label: '12:15', succeeded: 10, failed: 1 },
      { label: '12:30', succeeded: 12, failed: 0 },
      { label: '12:45', succeeded: 12, failed: 1 },
    ],
  },
  '24H': {
    timeRange: '24H',
    totalJobs: 1240,
    successRate: 97.1,
    avgDuration: 245,
    cpuLoad: 38,
    jobDistribution: { succeeded: 1205, failed: 23, cancelled: 12 },
    executionTrends: [
      { label: 'Mon', succeeded: 180, failed: 3 },
      { label: 'Tue', succeeded: 195, failed: 5 },
      { label: 'Wed', succeeded: 210, failed: 4 },
      { label: 'Thu', succeeded: 188, failed: 6 },
      { label: 'Fri', succeeded: 220, failed: 3 },
      { label: 'Sat', succeeded: 112, failed: 1 },
      { label: 'Sun', succeeded: 100, failed: 1 },
    ],
  },
  '7D': {
    timeRange: '7D',
    totalJobs: 8540,
    successRate: 96.8,
    avgDuration: 312,
    cpuLoad: 45,
    jobDistribution: { succeeded: 8267, failed: 189, cancelled: 84 },
    executionTrends: [
      { label: 'Week 1', succeeded: 1200, failed: 28 },
      { label: 'Week 2', succeeded: 1350, failed: 32 },
      { label: 'Week 3', succeeded: 1420, failed: 45 },
      { label: 'Week 4', succeeded: 1380, failed: 38 },
    ],
  },
  '30D': {
    timeRange: '30D',
    totalJobs: 35200,
    successRate: 95.4,
    avgDuration: 298,
    cpuLoad: 52,
    jobDistribution: { succeeded: 33580, failed: 1120, cancelled: 500 },
    executionTrends: [
      { label: 'Jan', succeeded: 8500, failed: 280 },
      { label: 'Feb', succeeded: 8200, failed: 310 },
      { label: 'Mar', succeeded: 8880, failed: 290 },
      { label: 'Apr', succeeded: 8000, failed: 240 },
    ],
  },
}

const USE_MOCK = import.meta.env.VITE_USE_MOCK === 'true' || !import.meta.env.VITE_API_URL

// Helper to create timestamp from Date
function createTimestamp(date: Date) {
  return new Timestamp({
    seconds: BigInt(Math.floor(date.getTime() / 1000)),
    nanos: (date.getTime() % 1000) * 1000000
  })
}

// Hooks
export function useSystemMetrics() {
  return useQuery({
    queryKey: metricsKeys.system(),
    queryFn: async () => {
      if (USE_MOCK) {
        await new Promise(resolve => setTimeout(resolve, 300))
        return mockSystemMetrics
      }

      try {
        const now = new Date()
        const yesterday = new Date(now.getTime() - 24 * 60 * 60 * 1000)

        // We need to fetch data from multiple sources to build the system metrics
        // explicit casting to avoid unknown types
        const [queueStatus, aggregated, providers] = await Promise.all([
          schedulerClient.getQueueStatus(new GetQueueStatusRequest({ schedulerName: 'default' })),
          metricsClient.getAggregatedMetrics(new MetricsQuery({
            fromTimestamp: createTimestamp(yesterday),
            toTimestamp: createTimestamp(now),
            metricTypes: ['cpu', 'memory']
          })),
          providerClient.listProviders(new ListProvidersRequest({}))
        ]) as [GetQueueStatusResponse, AggregatedMetrics, ListProvidersResponse]

        const status = queueStatus.status || new QueueStatus({})

        // Calculate active nodes from providers
        const activeNodes = providers.providers.length

        return {
          totalJobs: status.completedJobs + status.failedJobs + status.cancelledJobs + status.runningJobs + status.pendingJobs,
          runningJobs: status.runningJobs,
          failedJobs: status.failedJobs,
          succeededJobs: status.completedJobs,
          pendingJobs: status.pendingJobs,
          cpuLoad: aggregated.avgCpuUsage || 0,
          memoryUsage: aggregated.avgMemoryUsage || 0,
          uptime: 99.9,
          activeNodes: activeNodes,
          queueSize: status.pendingJobs
        }
      } catch (err) {
        console.error('Failed to fetch system metrics:', err)
        // Fallback to mock data on error
        return mockSystemMetrics
      }
    },
    refetchInterval: 10000, // Auto-refresh every 10 seconds
  })
}

export function useAggregatedMetrics(timeRange: '1H' | '24H' | '7D' | '30D' = '24H') {
  return useQuery({
    queryKey: metricsKeys.aggregated(timeRange),
    queryFn: async () => {
      if (USE_MOCK) {
        await new Promise(resolve => setTimeout(resolve, 500))
        return mockAggregatedMetrics[timeRange] || mockAggregatedMetrics['24H']
      }

      // Return mock data for now
      await new Promise(resolve => setTimeout(resolve, 500))
      return mockAggregatedMetrics[timeRange] || mockAggregatedMetrics['24H']
    },
  })
}
