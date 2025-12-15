import { describe, it, expect } from 'vitest'
import { renderHook, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { useJobs, useJob, useQueueJob, useCancelJob } from '@/hooks/useJobs'

const createWrapper = () => {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
    },
  })
  return ({ children }: { children: React.ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  )
}

describe('useJobs', () => {
  it('returns loading state initially', () => {
    const { result } = renderHook(() => useJobs(), { wrapper: createWrapper() })
    expect(result.current.isLoading).toBe(true)
  })

  it('returns jobs data after loading', async () => {
    const { result } = renderHook(() => useJobs(), { wrapper: createWrapper() })
    
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    
    expect(result.current.data).toBeDefined()
    expect(result.current.data?.length).toBeGreaterThan(0)
  })

  it('filters jobs by status', async () => {
    const { result } = renderHook(
      () => useJobs({ status: 'FAILED' }),
      { wrapper: createWrapper() }
    )
    
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    
    expect(result.current.data?.every(job => job.status === 'FAILED')).toBe(true)
  })

  it('filters jobs by search term', async () => {
    const { result } = renderHook(
      () => useJobs({ search: '5021' }),
      { wrapper: createWrapper() }
    )
    
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    
    expect(result.current.data?.some(job => job.id === '5021')).toBe(true)
  })
})

describe('useJob', () => {
  it('returns single job by id', async () => {
    const { result } = renderHook(
      () => useJob('5021'),
      { wrapper: createWrapper() }
    )
    
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    
    expect(result.current.data?.id).toBe('5021')
    expect(result.current.data?.name).toBe('AWS_Production_Build')
  })

  it('returns null for non-existent job', async () => {
    const { result } = renderHook(
      () => useJob('non-existent'),
      { wrapper: createWrapper() }
    )
    
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    
    expect(result.current.data).toBeNull()
  })

  it('does not fetch when id is empty', () => {
    const { result } = renderHook(
      () => useJob(''),
      { wrapper: createWrapper() }
    )
    
    expect(result.current.fetchStatus).toBe('idle')
  })
})

describe('useQueueJob', () => {
  it('creates a new job', async () => {
    const { result } = renderHook(
      () => useQueueJob(),
      { wrapper: createWrapper() }
    )
    
    result.current.mutate({ name: 'Test Job', command: 'echo hello' })
    
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    
    expect(result.current.data?.name).toBe('Test Job')
    expect(result.current.data?.status).toBe('PENDING')
  })
})

describe('useCancelJob', () => {
  it('cancels a job', async () => {
    const { result } = renderHook(
      () => useCancelJob(),
      { wrapper: createWrapper() }
    )
    
    result.current.mutate('5021')
    
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    
    expect(result.current.data?.success).toBe(true)
    expect(result.current.data?.jobId).toBe('5021')
  })
})
