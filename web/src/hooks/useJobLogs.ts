import { useQuery } from '@tanstack/react-query'
import { useState, useEffect, useCallback, useRef } from 'react'
import { logClient } from '@/lib/connect'

// Types
export type LogLevel = 'INFO' | 'WARN' | 'ERROR' | 'DEBUG'

export interface LogEntry {
  timestamp: string
  level: LogLevel
  message: string
  source?: string
  sequence?: number
}

// Query keys
export const logKeys = {
  all: ['logs'] as const,
  job: (jobId: string) => [...logKeys.all, 'job', jobId] as const,
}

// Helper to use mock data when backend is unavailable
const USE_MOCK = import.meta.env.VITE_USE_MOCK === 'true' || !import.meta.env.VITE_API_URL

// Mock log data
const generateMockLogs = (jobId: string): LogEntry[] => [
  { timestamp: '10:05:01', level: 'INFO', message: `[${jobId}] Starting job execution...` },
  { timestamp: '10:05:02', level: 'INFO', message: 'Pulling container image: node:18-alpine' },
  { timestamp: '10:05:05', level: 'INFO', message: 'Image pulled successfully' },
  { timestamp: '10:05:06', level: 'INFO', message: 'Creating container...' },
  { timestamp: '10:05:07', level: 'INFO', message: 'Container created: abc123def456' },
  { timestamp: '10:05:08', level: 'INFO', message: 'Starting container...' },
  { timestamp: '10:05:09', level: 'INFO', message: '> npm install' },
  { timestamp: '10:05:12', level: 'INFO', message: 'added 245 packages in 3.2s' },
  { timestamp: '10:05:13', level: 'INFO', message: '> npm run build' },
  { timestamp: '10:05:15', level: 'WARN', message: 'Warning: React version not specified in eslint-plugin-react settings' },
  { timestamp: '10:05:18', level: 'INFO', message: 'Compiled successfully!' },
  { timestamp: '10:05:19', level: 'INFO', message: 'Build output: 2.4 MB' },
  { timestamp: '10:05:20', level: 'INFO', message: '> npm run test' },
  { timestamp: '10:05:25', level: 'INFO', message: 'PASS src/App.test.tsx' },
  { timestamp: '10:05:26', level: 'INFO', message: 'PASS src/utils.test.tsx' },
  { timestamp: '10:05:27', level: 'ERROR', message: 'FAIL src/api.test.tsx - Expected 200 but got 404' },
  { timestamp: '10:05:28', level: 'WARN', message: 'Test suite failed. See above for details.' },
  { timestamp: '10:05:29', level: 'INFO', message: 'Tests: 2 passed, 1 failed, 3 total' },
  { timestamp: '10:05:30', level: 'INFO', message: 'Uploading artifacts...' },
  { timestamp: '10:05:32', level: 'INFO', message: 'Artifacts uploaded to s3://builds/job-5021/' },
]

// Hook for fetching logs (non-streaming)
export function useJobLogs(jobId: string) {
  return useQuery({
    queryKey: logKeys.job(jobId),
    queryFn: async () => {
      if (USE_MOCK) {
        await new Promise(resolve => setTimeout(resolve, 500))
        return generateMockLogs(jobId)
      }

      // Real gRPC call
      try {
        const response = await logClient.getLogs({
          jobId,
          limit: 1000,
        }) as { entries: Array<{ timestamp?: { toDate(): Date }, isStderr: boolean, line: string, sequence: bigint }> }
        
        return response.entries.map(entry => ({
          timestamp: entry.timestamp?.toDate().toLocaleTimeString() || '',
          level: entry.isStderr ? 'ERROR' as LogLevel : 'INFO' as LogLevel,
          message: entry.line,
          sequence: Number(entry.sequence),
        }))
      } catch (err) {
        console.warn('Failed to fetch logs from gRPC, using mock data:', err)
        return generateMockLogs(jobId)
      }
    },
    enabled: !!jobId,
  })
}

// Hook for streaming logs
export function useJobLogStream(jobId: string) {
  const [logs, setLogs] = useState<LogEntry[]>([])
  const [isStreaming, setIsStreaming] = useState(false)
  const [isPaused, setIsPaused] = useState(false)
  const [error, setError] = useState<Error | null>(null)
  const abortControllerRef = useRef<AbortController | null>(null)

  const startStreaming = useCallback(async () => {
    if (!jobId) return

    setIsStreaming(true)
    setError(null)

    if (USE_MOCK) {
      // Simulate streaming with mock data
      const mockLogs = generateMockLogs(jobId)
      let index = 0

      const interval = setInterval(() => {
        if (isPaused) return

        if (index < mockLogs.length) {
          setLogs(prev => [...prev, mockLogs[index]])
          index++
        } else {
          const newLog: LogEntry = {
            timestamp: new Date().toLocaleTimeString(),
            level: Math.random() > 0.9 ? 'WARN' : 'INFO',
            message: `Processing batch ${index - mockLogs.length + 1}...`,
          }
          setLogs(prev => [...prev, newLog])
          index++
        }
      }, 500)

      return () => {
        clearInterval(interval)
        setIsStreaming(false)
      }
    }

    // Real gRPC streaming - fallback to mock for now due to typing issues
    // TODO: Implement real streaming when types are properly configured
    console.warn('Real gRPC streaming not yet implemented, using mock')
    const mockLogs = generateMockLogs(jobId)
    let index = 0

    const interval = setInterval(() => {
      if (isPaused) return

      if (index < mockLogs.length) {
        setLogs(prev => [...prev, mockLogs[index]])
        index++
      } else {
        const newLog: LogEntry = {
          timestamp: new Date().toLocaleTimeString(),
          level: Math.random() > 0.9 ? 'WARN' : 'INFO',
          message: `Processing batch ${index - mockLogs.length + 1}...`,
        }
        setLogs(prev => [...prev, newLog])
        index++
      }
    }, 500)

    return () => {
      clearInterval(interval)
      setIsStreaming(false)
    }
  }, [jobId, isPaused])

  const stopStreaming = useCallback(() => {
    abortControllerRef.current?.abort()
    setIsStreaming(false)
  }, [])

  const togglePause = useCallback(() => {
    setIsPaused(prev => !prev)
  }, [])

  const clearLogs = useCallback(() => {
    setLogs([])
  }, [])

  // Auto-start streaming when jobId changes
  useEffect(() => {
    if (jobId) {
      startStreaming()
      return () => {
        abortControllerRef.current?.abort()
      }
    }
  }, [jobId, startStreaming])

  return {
    logs,
    isStreaming,
    isPaused,
    error,
    startStreaming,
    stopStreaming,
    togglePause,
    clearLogs,
  }
}
