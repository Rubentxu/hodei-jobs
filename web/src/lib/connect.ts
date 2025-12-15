import {
  JobExecutionService,
  LogStreamService,
  MetricsService,
  SchedulerService,
  WorkerAgentService,
} from '@/gen/hodei_all_in_one_connect'
import { ProviderManagementService } from '@/gen/provider_management_connect'
import { Code, ConnectError, createClient, type Interceptor } from '@connectrpc/connect'
import { createConnectTransport, createGrpcWebTransport } from '@connectrpc/connect-web'

// Configuration
const API_URL = import.meta.env.VITE_API_URL || 'http://localhost:50051'
const MAX_RETRIES = 3
const RETRY_DELAY_MS = 1000
const REQUEST_TIMEOUT_MS = 30000

// Error types for better error handling
export class GrpcConnectionError extends Error {
  constructor(
    message: string,
    public readonly code: Code,
    public readonly retryable: boolean
  ) {
    super(message)
    this.name = 'GrpcConnectionError'
  }
}

// Logging interceptor for debugging
const loggingInterceptor: Interceptor = (next) => async (req) => {
  const startTime = performance.now()
  const method = `${req.service.typeName}/${req.method.name}`

  if (import.meta.env.DEV) {
    console.debug(`[gRPC] → ${method}`, req.message)
  }

  try {
    const res = await next(req)
    const duration = Math.round(performance.now() - startTime)

    if (import.meta.env.DEV) {
      console.debug(`[gRPC] ← ${method} (${duration}ms)`, res.message)
    }

    return res
  } catch (err) {
    const duration = Math.round(performance.now() - startTime)
    console.error(`[gRPC] ✗ ${method} (${duration}ms)`, err)
    throw err
  }
}

// Retry interceptor with exponential backoff
const retryInterceptor: Interceptor = (next) => async (req) => {
  let lastError: unknown

  for (let attempt = 0; attempt <= MAX_RETRIES; attempt++) {
    try {
      return await next(req)
    } catch (err) {
      lastError = err

      // Don't retry non-retryable errors
      if (err instanceof ConnectError) {
        const retryableCodes = [
          Code.Unavailable,
          Code.ResourceExhausted,
          Code.Aborted,
          Code.DeadlineExceeded,
        ]

        if (!retryableCodes.includes(err.code)) {
          throw err
        }
      }

      // Don't retry on last attempt
      if (attempt === MAX_RETRIES) {
        break
      }

      // Exponential backoff
      const delay = RETRY_DELAY_MS * Math.pow(2, attempt)
      if (import.meta.env.DEV) {
        console.warn(`[gRPC] Retry ${attempt + 1}/${MAX_RETRIES} after ${delay}ms`)
      }
      await new Promise(resolve => setTimeout(resolve, delay))
    }
  }

  throw lastError
}

// Timeout interceptor
const timeoutInterceptor: Interceptor = (next) => async (req) => {
  const controller = new AbortController()
  const timeoutId = setTimeout(() => controller.abort(), REQUEST_TIMEOUT_MS)

  try {
    // Merge abort signals if one already exists
    const originalSignal = req.signal
    if (originalSignal) {
      originalSignal.addEventListener('abort', () => controller.abort())
    }

    return await next({
      ...req,
      signal: controller.signal,
    })
  } finally {
    clearTimeout(timeoutId)
  }
}

// Create transport with gRPC-Web support
export const transport = createGrpcWebTransport({
  baseUrl: API_URL,
  interceptors: [
    loggingInterceptor,
    retryInterceptor,
    timeoutInterceptor,
  ],
})

// Alternative Connect transport (for servers that support it)
export const connectTransport = createConnectTransport({
  baseUrl: API_URL,
  interceptors: [
    loggingInterceptor,
    retryInterceptor,
    timeoutInterceptor,
  ],
})

// Service clients
export const jobClient = createClient(JobExecutionService, transport)
export const logClient = createClient(LogStreamService, transport)
export const metricsClient = createClient(MetricsService, transport)
export const schedulerClient = createClient(SchedulerService, transport)
export const workerClient = createClient(WorkerAgentService, transport)
export const providerClient = createClient(ProviderManagementService, transport)

// Helper to check if backend is available
export async function checkBackendHealth(): Promise<boolean> {
  try {
    // Try to get queue status as a health check
    await schedulerClient.getQueueStatus({ schedulerName: 'default' })
    return true
  } catch (err) {
    if (err instanceof ConnectError) {
      console.warn('[gRPC] Backend health check failed:', err.message)
    }
    return false
  }
}

// Helper to convert ConnectError to user-friendly message
export function getErrorMessage(err: unknown): string {
  if (err instanceof ConnectError) {
    switch (err.code) {
      case Code.Unavailable:
        return 'El servidor no está disponible. Por favor, inténtalo de nuevo más tarde.'
      case Code.DeadlineExceeded:
        return 'La solicitud tardó demasiado. Por favor, inténtalo de nuevo.'
      case Code.PermissionDenied:
        return 'No tienes permisos para realizar esta acción.'
      case Code.NotFound:
        return 'El recurso solicitado no existe.'
      case Code.InvalidArgument:
        return 'Los datos proporcionados no son válidos.'
      case Code.AlreadyExists:
        return 'El recurso ya existe.'
      case Code.ResourceExhausted:
        return 'Se ha alcanzado el límite de solicitudes. Por favor, espera un momento.'
      default:
        return err.message || 'Ha ocurrido un error inesperado.'
    }
  }

  if (err instanceof Error) {
    return err.message
  }

  return 'Ha ocurrido un error desconocido.'
}
