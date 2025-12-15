import { useState } from 'react'
import { Link } from 'react-router-dom'
import { useProviders, type ProviderStatus } from '@/hooks/useProviders'

type FilterId = 'all' | 'active' | 'unhealthy'

export function ProvidersPage() {
  const [filter, setFilter] = useState<FilterId>('all')
  const [search, setSearch] = useState('')
  
  const statusFilter: ProviderStatus | undefined = 
    filter === 'active' ? 'ACTIVE' : 
    filter === 'unhealthy' ? 'UNHEALTHY' : 
    undefined

  const { data: providers, isLoading } = useProviders({ 
    status: statusFilter,
    search: search || undefined 
  })

  const getProviderIcon = (type: string) => {
    switch (type) {
      case 'KUBERNETES': return 'hub'
      case 'AZURE_VM': return 'cloud'
      default: return 'directions_boat'
    }
  }

  const getProviderIconBg = (type: string) => {
    switch (type) {
      case 'KUBERNETES': return 'bg-indigo-50 text-indigo-600'
      case 'AZURE_VM': return 'bg-purple-50 text-purple-600'
      default: return 'bg-blue-50 text-blue-600'
    }
  }

  const getStatusBadge = (status: ProviderStatus) => {
    switch (status) {
      case 'ACTIVE':
        return 'bg-green-50 text-green-700 border-green-100'
      case 'UNHEALTHY':
        return 'bg-red-50 text-red-700 border-red-100'
      case 'OVERLOADED':
        return 'bg-amber-50 text-amber-700 border-amber-100'
      default:
        return 'bg-slate-50 text-slate-700 border-slate-100'
    }
  }

  const getStatusLabel = (status: ProviderStatus) => {
    switch (status) {
      case 'ACTIVE': return 'Active'
      case 'UNHEALTHY': return 'Unhealthy'
      case 'OVERLOADED': return 'Overloaded'
      default: return 'Offline'
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
    <div className="text-slate-800 antialiased min-h-screen flex flex-col bg-[#f8fafc]">
      {/* Header */}
      <header className="bg-white sticky top-0 z-20 border-b border-slate-200 shadow-sm">
        <div className="h-11 w-full"></div>
        <div className="px-4 py-3 flex items-center justify-between">
          <h1 className="text-xl font-bold tracking-tight">Providers</h1>
          <Link 
            to="/providers/new"
            className="p-2 rounded-full hover:bg-slate-100 text-primary"
          >
            <span className="material-symbols-outlined font-bold">add</span>
          </Link>
        </div>
        <div className="px-4 pb-4 space-y-3">
          {/* Search */}
          <div className="relative">
            <span className="absolute inset-y-0 left-0 flex items-center pl-3 text-slate-400">
              <span className="material-symbols-outlined text-[20px]">search</span>
            </span>
            <input
              type="text"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              className="w-full pl-10 pr-4 py-2 bg-slate-100 border-none rounded-xl text-sm focus:ring-2 focus:ring-primary focus:bg-white transition-all placeholder-slate-500"
              placeholder="Search providers..."
            />
          </div>
          
          {/* Filter Chips */}
          <div className="flex gap-2 overflow-x-auto pb-1 no-scrollbar -mx-4 px-4">
            <button
              onClick={() => setFilter('all')}
              className={`flex items-center gap-1 px-3 py-1.5 rounded-full text-xs font-medium whitespace-nowrap shadow-sm ${
                filter === 'all'
                  ? 'bg-slate-800 text-white'
                  : 'bg-white border border-slate-200 text-slate-600'
              }`}
            >
              All
            </button>
            <button
              onClick={() => setFilter('active')}
              className={`flex items-center gap-1 px-3 py-1.5 rounded-full text-xs font-medium whitespace-nowrap shadow-sm ${
                filter === 'active'
                  ? 'bg-slate-800 text-white'
                  : 'bg-white border border-slate-200 text-slate-600'
              }`}
            >
              <span className="w-1.5 h-1.5 rounded-full bg-green-500"></span>
              Active
            </button>
            <button
              onClick={() => setFilter('unhealthy')}
              className={`flex items-center gap-1 px-3 py-1.5 rounded-full text-xs font-medium whitespace-nowrap shadow-sm ${
                filter === 'unhealthy'
                  ? 'bg-slate-800 text-white'
                  : 'bg-white border border-slate-200 text-slate-600'
              }`}
            >
              <span className="w-1.5 h-1.5 rounded-full bg-red-500"></span>
              Unhealthy
            </button>
            <button className="flex items-center gap-1 px-3 py-1.5 bg-white border border-slate-200 text-slate-600 rounded-full text-xs font-medium whitespace-nowrap shadow-sm">
              <span className="material-symbols-outlined text-[14px]">filter_list</span>
              More Filters
            </button>
          </div>
        </div>
      </header>

      {/* Main Content */}
      <main className="flex-1 px-4 py-4 space-y-4">
        {providers?.map((provider) => (
          <Link
            key={provider.id}
            to={`/providers/${provider.id}`}
            className="block bg-white rounded-2xl shadow-sm border border-slate-100 active:scale-[0.98] transition-transform duration-100 overflow-hidden relative"
          >
            <div className="absolute top-0 right-0 p-4">
              <span className="material-symbols-outlined text-slate-300">chevron_right</span>
            </div>
            <div className="p-4 space-y-3">
              <div className="flex items-center gap-3">
                <div className={`w-10 h-10 rounded-xl flex items-center justify-center ${getProviderIconBg(provider.type)}`}>
                  <span className="material-symbols-outlined">{getProviderIcon(provider.type)}</span>
                </div>
                <div>
                  <div className="flex items-center gap-2">
                    <h3 className="font-semibold text-sm">{provider.name}</h3>
                    <span className={`px-2 py-0.5 rounded-md text-[10px] font-bold uppercase tracking-wide border ${getStatusBadge(provider.status)}`}>
                      {getStatusLabel(provider.status)}
                    </span>
                  </div>
                  <p className="text-xs text-slate-500 font-mono mt-0.5">ID: {provider.id}</p>
                </div>
              </div>
              
              <div className="grid grid-cols-2 gap-2 pt-2 border-t border-slate-50">
                <div className="flex flex-col">
                  <span className="text-[10px] uppercase text-slate-400 font-semibold tracking-wider">Type</span>
                  <span className="text-xs font-medium text-slate-700">
                    {provider.type === 'DOCKER' ? 'Docker' : 
                     provider.type === 'KUBERNETES' ? 'Kubernetes' : 
                     'Azure VM'}
                  </span>
                </div>
                {provider.status === 'OVERLOADED' ? (
                  <div className="flex flex-col">
                    <span className="text-[10px] uppercase text-slate-400 font-semibold tracking-wider">Load</span>
                    <div className="flex items-center gap-2">
                      <div className="flex-1 h-1.5 bg-slate-200 rounded-full overflow-hidden">
                        <div 
                          className="h-full bg-amber-500" 
                          style={{ width: `${provider.cpuUsage}%` }}
                        ></div>
                      </div>
                      <span className="text-[10px] text-amber-600 font-bold">{provider.cpuUsage}%</span>
                    </div>
                  </div>
                ) : (
                  <div className="flex flex-col">
                    <span className="text-[10px] uppercase text-slate-400 font-semibold tracking-wider">Resources</span>
                    <span className="text-xs font-medium text-slate-700">
                      {provider.maxConcurrency || 8} vCPU / {(provider.maxConcurrency || 8) * 2}GB
                    </span>
                  </div>
                )}
              </div>

              {/* Warning for unhealthy providers */}
              {provider.status === 'UNHEALTHY' && (
                <div className="flex items-start gap-2 bg-red-50 p-2 rounded-lg">
                  <span className="material-symbols-outlined text-red-500 text-[16px] mt-0.5">warning</span>
                  <p className="text-[11px] text-red-700 leading-tight">
                    Connection timeout. Check network config.
                  </p>
                </div>
              )}
            </div>
          </Link>
        ))}

        {providers?.length === 0 && (
          <div className="text-center py-12 text-slate-500">
            <span className="material-symbols-outlined text-4xl mb-2">dns</span>
            <p>No providers found</p>
          </div>
        )}
      </main>

      {/* Sort FAB */}
      <div className="fixed bottom-20 right-4 z-20">
        <button className="bg-slate-800 text-white p-3 rounded-full shadow-lg flex items-center justify-center active:scale-95 transition-transform">
          <span className="material-symbols-outlined">sort</span>
        </button>
      </div>
    </div>
  )
}
