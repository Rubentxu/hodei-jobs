import { useParams, useNavigate } from 'react-router-dom'
import { useProvider } from '@/hooks/useProviders'

interface HealthLogEntry {
  type: 'success' | 'error' | 'warning'
  title: string
  description: string
  time: string
}

export function ProviderDetailPage() {
  const { providerId } = useParams<{ providerId: string }>()
  const navigate = useNavigate()
  const { data: provider, isLoading } = useProvider(providerId || '')

  const healthLog: HealthLogEntry[] = [
    { type: 'success', title: 'Health Check Passed', description: 'Latency 45ms, Disk Space OK', time: '2m ago' },
    { type: 'success', title: 'Job Completed Successfully', description: 'Job #4921 finished in 12s', time: '15m ago' },
    { type: 'error', title: 'Connection Timeout', description: 'Agent failed to heartbeat', time: '1h ago' },
    { type: 'warning', title: 'High Memory Usage', description: 'Memory usage > 85%', time: '3h ago' },
  ]

  const capabilities = ['docker', 'java-17', 'python-3.11', 'gpu-cuda']

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-screen bg-[#F2F2F7]">
        <span className="material-symbols-outlined animate-spin text-[#007AFF] text-4xl">sync</span>
      </div>
    )
  }

  return (
    <div className="bg-[#F2F2F7] min-h-screen pb-20">
      {/* iOS Status Bar */}
      <div className="h-12 w-full bg-[#F2F2F7] sticky top-0 z-50 flex items-end px-6 pb-2 justify-between text-black text-xs font-semibold">
        <span>9:41</span>
        <div className="flex gap-1.5 items-center">
          <span className="material-symbols-outlined text-sm font-bold">signal_cellular_alt</span>
          <span className="material-symbols-outlined text-sm font-bold">wifi</span>
          <span className="material-symbols-outlined text-lg font-bold rotate-90">battery_full</span>
        </div>
      </div>

      {/* Navigation Bar */}
      <div className="bg-[#F2F2F7] px-4 py-2 flex items-center justify-between sticky top-12 z-40 border-b border-gray-300/50 backdrop-blur-xl bg-opacity-90">
        <button 
          onClick={() => navigate(-1)}
          className="flex items-center text-[#007AFF] -ml-2"
        >
          <span className="material-symbols-outlined text-3xl">chevron_left</span>
          <span className="text-[17px] font-normal">Providers</span>
        </button>
        <h1 className="text-[17px] font-semibold text-black absolute left-1/2 transform -translate-x-1/2">
          {provider?.name || 'Provider'}
        </h1>
        <button className="text-[#007AFF] text-[17px]">Edit</button>
      </div>

      <main className="px-4 py-6 space-y-6">
        {/* Provider Header Card */}
        <div className="bg-white rounded-[10px] overflow-hidden shadow-sm">
          <div className="p-4 flex items-start justify-between border-b border-gray-100">
            <div className="flex items-center gap-4">
              <div className="w-14 h-14 rounded-full bg-blue-50 flex items-center justify-center text-blue-600">
                <span className="material-symbols-outlined text-3xl">dns</span>
              </div>
              <div>
                <h2 className="text-xl font-bold text-gray-900">{provider?.name}</h2>
                <p className="text-sm text-gray-500">ID: {provider?.id}</p>
              </div>
            </div>
            <span className={`inline-flex items-center gap-1 px-2.5 py-1 rounded-full text-xs font-medium ${
              provider?.status === 'ACTIVE' 
                ? 'bg-green-100 text-green-700' 
                : provider?.status === 'UNHEALTHY'
                ? 'bg-red-100 text-red-700'
                : 'bg-orange-100 text-orange-700'
            }`}>
              <span className={`w-1.5 h-1.5 rounded-full ${
                provider?.status === 'ACTIVE' ? 'bg-green-600' : 
                provider?.status === 'UNHEALTHY' ? 'bg-red-600' : 'bg-orange-600'
              }`}></span>
              {provider?.status === 'ACTIVE' ? 'Online' : 
               provider?.status === 'UNHEALTHY' ? 'Unhealthy' : 'Overloaded'}
            </span>
          </div>
          <div className="grid grid-cols-2 divide-x divide-gray-100 bg-gray-50/50">
            <div className="p-4 text-center">
              <span className="block text-xs uppercase tracking-wide text-gray-400 font-semibold mb-1">Current Jobs</span>
              <span className="text-2xl font-bold text-gray-900">{provider?.currentJobs || 0}</span>
              <span className="text-xs text-gray-500 block mt-1">/ {provider?.maxConcurrency || 20} Capacity</span>
            </div>
            <div className="p-4 text-center">
              <span className="block text-xs uppercase tracking-wide text-gray-400 font-semibold mb-1">Health Score</span>
              <span className={`text-2xl font-bold ${
                (provider?.healthScore || 0) >= 90 ? 'text-green-600' : 
                (provider?.healthScore || 0) >= 70 ? 'text-orange-600' : 'text-red-600'
              }`}>{provider?.healthScore || 0}%</span>
              <span className="text-xs text-gray-500 block mt-1">Last 24h</span>
            </div>
          </div>
        </div>

        {/* Action Buttons */}
        <div className="grid grid-cols-3 gap-3">
          <button className="flex flex-col items-center justify-center bg-white p-3 rounded-[10px] shadow-sm active:bg-gray-50 transition-colors">
            <div className="w-10 h-10 rounded-full bg-green-100 flex items-center justify-center text-green-600 mb-2">
              <span className="material-symbols-outlined">check_circle</span>
            </div>
            <span className="text-xs font-medium text-gray-900">Mark Healthy</span>
          </button>
          <button className="flex flex-col items-center justify-center bg-white p-3 rounded-[10px] shadow-sm active:bg-gray-50 transition-colors">
            <div className="w-10 h-10 rounded-full bg-orange-100 flex items-center justify-center text-orange-600 mb-2">
              <span className="material-symbols-outlined">construction</span>
            </div>
            <span className="text-xs font-medium text-gray-900">Maintenance</span>
          </button>
          <button className="flex flex-col items-center justify-center bg-white p-3 rounded-[10px] shadow-sm active:bg-gray-50 transition-colors">
            <div className="w-10 h-10 rounded-full bg-red-100 flex items-center justify-center text-red-600 mb-2">
              <span className="material-symbols-outlined">power_settings_new</span>
            </div>
            <span className="text-xs font-medium text-gray-900">Shutdown</span>
          </button>
        </div>

        {/* Configuration */}
        <div className="space-y-2">
          <h3 className="text-xs uppercase text-gray-500 font-semibold ml-4 mb-2">Configuration</h3>
          <div className="bg-white rounded-[10px] overflow-hidden shadow-sm divide-y divide-gray-100">
            <div className="flex items-center justify-between p-4 active:bg-gray-50">
              <span className="text-[15px] text-gray-900">Type</span>
              <span className="text-[15px] text-gray-500">
                {provider?.type === 'DOCKER' ? 'Docker' : 
                 provider?.type === 'KUBERNETES' ? 'Kubernetes' : 'Azure VM'}
              </span>
            </div>
            <div className="flex items-center justify-between p-4 active:bg-gray-50">
              <span className="text-[15px] text-gray-900">Region</span>
              <span className="text-[15px] text-gray-500">{provider?.region || 'us-east-1'}</span>
            </div>
            <div className="flex items-center justify-between p-4 active:bg-gray-50">
              <span className="text-[15px] text-gray-900">OS Image</span>
              <span className="text-[15px] text-gray-500">Ubuntu 22.04 LTS</span>
            </div>
            <div className="flex items-center justify-between p-4 active:bg-gray-50">
              <span className="text-[15px] text-gray-900">Max Concurrency</span>
              <span className="text-[15px] text-gray-500">{provider?.maxConcurrency || 20}</span>
            </div>
          </div>
        </div>

        {/* Capabilities */}
        <div className="space-y-2">
          <h3 className="text-xs uppercase text-gray-500 font-semibold ml-4 mb-2">Capabilities</h3>
          <div className="bg-white rounded-[10px] p-4 shadow-sm flex flex-wrap gap-2">
            {(provider?.capabilities || capabilities).map((cap, index) => (
              <span 
                key={index}
                className="inline-flex items-center px-2.5 py-1 rounded-md text-xs font-medium bg-blue-50 text-blue-700 border border-blue-100"
              >
                {cap}
              </span>
            ))}
            <span className="inline-flex items-center px-2.5 py-1 rounded-md text-xs font-medium bg-gray-100 text-gray-600 border border-gray-200">
              + 4 more
            </span>
          </div>
        </div>

        {/* Health Log */}
        <div className="space-y-2">
          <h3 className="text-xs uppercase text-gray-500 font-semibold ml-4 mb-2">Health Log</h3>
          <div className="bg-white rounded-[10px] overflow-hidden shadow-sm divide-y divide-gray-100">
            {healthLog.map((entry, index) => (
              <div 
                key={index} 
                className={`p-4 flex gap-3 ${entry.type === 'error' ? 'bg-red-50/30' : ''}`}
              >
                <div className="mt-1">
                  <span className={`material-symbols-outlined text-lg ${
                    entry.type === 'success' ? 'text-green-500' :
                    entry.type === 'error' ? 'text-red-500' : 'text-orange-500'
                  }`}>
                    {entry.type === 'success' ? 'check_circle' :
                     entry.type === 'error' ? 'error' : 'warning'}
                  </span>
                </div>
                <div className="flex-1">
                  <p className="text-[15px] font-medium text-gray-900">{entry.title}</p>
                  <p className="text-[13px] text-gray-500 mt-0.5">{entry.description}</p>
                </div>
                <span className="text-xs text-gray-400">{entry.time}</span>
              </div>
            ))}
            <button className="w-full py-3 text-[15px] text-[#007AFF] font-medium hover:bg-gray-50 transition-colors">
              View Full Log
            </button>
          </div>
        </div>

        {/* Raw Config */}
        <div className="space-y-2">
          <h3 className="text-xs uppercase text-gray-500 font-semibold ml-4 mb-2">Raw Config</h3>
          <div className="bg-white rounded-[10px] p-4 shadow-sm">
            <pre className="text-[11px] font-mono text-gray-600 overflow-x-auto bg-gray-50 p-3 rounded border border-gray-100">
{JSON.stringify({
  provider_id: provider?.id || 'p-8f32a19b',
  auto_scale: true,
  idle_timeout: 300,
  labels: provider?.labels ? Object.keys(provider.labels) : ['prod', 'us-east'],
  security_group: 'sg-0a1b2c3d'
}, null, 2)}
            </pre>
          </div>
        </div>
      </main>
    </div>
  )
}
