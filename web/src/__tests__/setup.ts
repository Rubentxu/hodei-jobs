import '@testing-library/jest-dom/vitest'
import { cleanup } from '@testing-library/react'
import { afterEach, vi } from 'vitest'

afterEach(() => {
  cleanup()
})

// Mock @bufbuild/protobuf to avoid ESM issues in tests
vi.mock('@bufbuild/protobuf', () => ({
  create: vi.fn((_schema: unknown, data: unknown) => data),
}))

// Mock the generated protobuf files
vi.mock('@/gen/hodei_all_in_one_pb', () => ({
  JobDefinitionSchema: {},
  JobIdSchema: {},
}))

// Mock the connect clients
vi.mock('@/lib/connect', () => ({
  transport: {},
  jobClient: {
    queueJob: vi.fn().mockResolvedValue({ success: true }),
    cancelJob: vi.fn().mockResolvedValue({ success: true }),
  },
  logClient: {
    getLogs: vi.fn().mockResolvedValue({ entries: [] }),
    subscribeLogs: vi.fn(),
  },
  metricsClient: {
    getAggregatedMetrics: vi.fn().mockResolvedValue({}),
  },
  schedulerClient: {
    getQueueStatus: vi.fn().mockResolvedValue({}),
  },
}))

// Mock window.matchMedia for responsive tests
Object.defineProperty(window, 'matchMedia', {
  writable: true,
  value: vi.fn().mockImplementation((query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(),
  })),
})

// Mock IntersectionObserver
class MockIntersectionObserver {
  observe = vi.fn()
  disconnect = vi.fn()
  unobserve = vi.fn()
}

Object.defineProperty(window, 'IntersectionObserver', {
  writable: true,
  value: MockIntersectionObserver,
})

// Mock ResizeObserver
class MockResizeObserver {
  observe = vi.fn()
  disconnect = vi.fn()
  unobserve = vi.fn()
}

Object.defineProperty(window, 'ResizeObserver', {
  writable: true,
  value: MockResizeObserver,
})
