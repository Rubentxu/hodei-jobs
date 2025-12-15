
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

// Types - will be replaced by generated types from proto
import {
  GetProviderRequest,
  GetProviderResponse,
  ListProvidersRequest,
  ListProvidersResponse,
  ProviderStatus as ProtoProviderStatus,
  ProviderType as ProtoProviderType,
  RegisterProviderRequest,
  RegisterProviderResponse,
  UpdateProviderRequest,
  UpdateProviderResponse
} from '@/gen/provider_management_pb'
import { providerClient } from '@/lib/connect'

// Types - will be replaced by generated types from proto
export type ProviderStatus = 'ACTIVE' | 'UNHEALTHY' | 'OVERLOADED' | 'OFFLINE'
export type ProviderType = 'DOCKER' | 'KUBERNETES' | 'AZURE_VM'

function mapProviderStatus(status: ProtoProviderStatus): ProviderStatus {
  switch (status) {
    case ProtoProviderStatus.ACTIVE: return 'ACTIVE';
    case ProtoProviderStatus.UNHEALTHY: return 'UNHEALTHY';
    case ProtoProviderStatus.OVERLOADED: return 'OVERLOADED';
    case ProtoProviderStatus.DISABLED: return 'OFFLINE';
    case ProtoProviderStatus.MAINTENANCE: return 'OFFLINE';
    default: return 'OFFLINE';
  }
}

function mapProviderType(type: ProtoProviderType): ProviderType {
  switch (type) {
    case ProtoProviderType.DOCKER: return 'DOCKER';
    case ProtoProviderType.KUBERNETES: return 'KUBERNETES';
    case ProtoProviderType.AZURE_VMS: return 'AZURE_VM';
    default: return 'DOCKER';
  }
}

export interface Provider {
  id: string
  name: string
  status: ProviderStatus
  type: ProviderType
  endpoint?: string
  region?: string
  cpuUsage?: number
  memoryUsage?: number
  currentJobs?: number
  healthScore?: number
  maxConcurrency?: number
  capabilities?: string[]
  labels?: Record<string, string>
}

export interface ProvidersFilter {
  status?: ProviderStatus
  type?: ProviderType
  search?: string
}

// Query keys
export const providerKeys = {
  all: ['providers'] as const,
  lists: () => [...providerKeys.all, 'list'] as const,
  list: (filters: ProvidersFilter) => [...providerKeys.lists(), filters] as const,
  details: () => [...providerKeys.all, 'detail'] as const,
  detail: (id: string) => [...providerKeys.details(), id] as const,
}

// Hooks
export function useProviders(filters: ProvidersFilter = {}) {
  return useQuery({
    queryKey: providerKeys.list(filters),
    queryFn: async () => {
      // Real gRPC call
      try {
        const request = new ListProvidersRequest({
          providerType: filters.type ? (filters.type === 'DOCKER' ? ProtoProviderType.DOCKER : filters.type === 'KUBERNETES' ? ProtoProviderType.KUBERNETES : ProtoProviderType.AZURE_VMS) : undefined,
          // status mapping if needed
        })

        const response = await providerClient.listProviders(request) as ListProvidersResponse

        let providers = response.providers.map(p => ({
          id: p.id,
          name: p.name,
          status: mapProviderStatus(p.status),
          type: mapProviderType(p.providerType),
          endpoint: p.typeConfig?.config.case === 'docker' ? p.typeConfig.config.value.socketPath :
            p.typeConfig?.config.case === 'kubernetes' ? p.typeConfig.config.value.kubeconfigPath : undefined,
          region: p.capabilities?.regions[0],
          cpuUsage: 0, // Not in ProviderConfig, need stats
          memoryUsage: 0,
          currentJobs: p.activeWorkers,
          healthScore: p.status === ProtoProviderStatus.ACTIVE ? 100 : 50,
          maxConcurrency: p.maxWorkers,
          capabilities: p.capabilities?.runtimes || [],
          labels: p.metadata || {},
        }))

        if (filters.search) {
          const search = filters.search.toLowerCase()
          providers = providers.filter(p =>
            p.id.toLowerCase().includes(search) ||
            p.name.toLowerCase().includes(search)
          )
        }

        return providers
      } catch (err) {
        console.error('Failed to list providers:', err)
        return []
      }
    },
  })
}

export function useProvider(id: string) {
  return useQuery({
    queryKey: providerKeys.detail(id),
    queryFn: async () => {
      // Real gRPC call
      try {
        const request = new GetProviderRequest({
          providerId: id
        })

        const response = await providerClient.getProvider(request) as GetProviderResponse
        const p = response.provider

        if (!p) return null

        return {
          id: p.id,
          name: p.name,
          status: mapProviderStatus(p.status),
          type: mapProviderType(p.providerType),
          endpoint: p.typeConfig?.config.case === 'docker' ? p.typeConfig.config.value.socketPath :
            p.typeConfig?.config.case === 'kubernetes' ? p.typeConfig.config.value.kubeconfigPath : undefined,
          region: p.capabilities?.regions[0],
          cpuUsage: 0,
          memoryUsage: 0,
          currentJobs: p.activeWorkers,
          healthScore: p.status === ProtoProviderStatus.ACTIVE ? 100 : 50,
          maxConcurrency: p.maxWorkers,
          capabilities: p.capabilities?.runtimes || [],
          labels: p.metadata || {},
        }
      } catch (err) {
        console.error('Failed to get provider:', err)
        return null
      }
    },
    enabled: !!id,
  })
}

export function useRegisterProvider() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: async (providerData: Partial<Provider>) => {
      try {
        const request = new RegisterProviderRequest({
          name: providerData.name,
          providerType: providerData.type === 'DOCKER' ? ProtoProviderType.DOCKER :
            providerData.type === 'KUBERNETES' ? ProtoProviderType.KUBERNETES : ProtoProviderType.AZURE_VMS,
          // Add other fields as needed
        })

        const response = await providerClient.registerProvider(request) as RegisterProviderResponse
        return response.provider
      } catch (err) {
        console.error('Failed to register provider:', err)
        throw err
      }
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: providerKeys.lists() })
    },
  })
}

export function useUpdateProvider() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: async ({ id, ...data }: Partial<Provider> & { id: string }) => {
      try {
        // 1. Fetch existing provider
        const getRequest = new GetProviderRequest({ providerId: id })
        const getResponse = await providerClient.getProvider(getRequest) as GetProviderResponse
        const existingProvider = getResponse.provider

        if (!existingProvider) {
          throw new Error(`Provider ${id} not found`)
        }

        // 2. Merge changes
        // Note: This is a simplified merge. For complex nested objects like typeConfig, 
        // we might need more logic if we want to support partial updates of those.
        // For now, we assume 'data' contains top-level fields we want to update.

        if (data.name) existingProvider.name = data.name
        if (data.status) {
          existingProvider.status = data.status === 'ACTIVE' ? ProtoProviderStatus.ACTIVE :
            data.status === 'UNHEALTHY' ? ProtoProviderStatus.UNHEALTHY :
              data.status === 'OVERLOADED' ? ProtoProviderStatus.OVERLOADED :
                ProtoProviderStatus.DISABLED // Default to DISABLED for OFFLINE
        }
        if (data.maxConcurrency !== undefined) existingProvider.maxWorkers = data.maxConcurrency

        // Update labels/metadata
        if (data.labels) {
          existingProvider.metadata = { ...existingProvider.metadata, ...data.labels }
        }

        // 3. Send update
        const updateRequest = new UpdateProviderRequest({
          provider: existingProvider
        })

        const response = await providerClient.updateProvider(updateRequest) as UpdateProviderResponse
        return response.provider
      } catch (err) {
        console.error('Failed to update provider:', err)
        throw err
      }
    },
    onSuccess: (_, variables) => {
      queryClient.invalidateQueries({ queryKey: providerKeys.detail(variables.id) })
      queryClient.invalidateQueries({ queryKey: providerKeys.lists() })
    },
  })
}
