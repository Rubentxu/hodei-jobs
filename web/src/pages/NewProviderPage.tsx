import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useRegisterProvider, type ProviderType } from '@/hooks/useProviders'

export function NewProviderPage() {
  const navigate = useNavigate()
  const registerProvider = useRegisterProvider()

  const [providerType, setProviderType] = useState<ProviderType>('DOCKER')
  const [name, setName] = useState('')
  const [endpoint, setEndpoint] = useState('')
  const [apiToken, setApiToken] = useState('')
  const [labels, setLabels] = useState('')
  const [maxMemory, setMaxMemory] = useState(16)
  const [maxCpus, setMaxCpus] = useState(4)
  const [gpuSupport, setGpuSupport] = useState(false)
  const [secureBoot, setSecureBoot] = useState(true)
  const [showPassword, setShowPassword] = useState(false)

  const handleSubmit = async () => {
    if (!name.trim()) {
      alert('Please enter a provider name')
      return
    }

    await registerProvider.mutateAsync({
      name,
      type: providerType,
      endpoint,
      maxConcurrency: maxCpus,
      capabilities: gpuSupport ? ['gpu', 'cuda'] : [],
      labels: labels ? Object.fromEntries(labels.split(',').map(l => [l.trim(), 'true'])) : {},
    })

    navigate('/providers')
  }

  return (
    <div className="bg-[#f1f5f9] min-h-screen text-slate-800 pb-24">
      {/* Navigation */}
      <nav className="sticky top-0 z-50 bg-white/90 backdrop-blur-md border-b border-slate-200 px-4 py-3 flex items-center justify-between">
        <div className="flex items-center gap-3">
          <button 
            onClick={() => navigate(-1)}
            className="material-symbols-outlined text-slate-500 hover:text-slate-800 transition-colors"
          >
            arrow_back_ios_new
          </button>
          <h1 className="text-lg font-semibold tracking-tight">New Provider</h1>
        </div>
        <button 
          onClick={handleSubmit}
          className="text-primary font-medium text-sm"
        >
          Save
        </button>
      </nav>

      <main className="max-w-md mx-auto p-4 space-y-6">
        {/* Provider Type Section */}
        <section className="bg-white rounded-2xl p-5 shadow-sm border border-slate-100">
          <h2 className="text-sm font-semibold text-slate-500 uppercase tracking-wider mb-4">Provider Type</h2>
          <div className="grid grid-cols-3 gap-3">
            <label className="cursor-pointer group">
              <input
                type="radio"
                name="provider_type"
                value="DOCKER"
                checked={providerType === 'DOCKER'}
                onChange={() => setProviderType('DOCKER')}
                className="peer sr-only"
              />
              <div className="flex flex-col items-center justify-center p-3 rounded-xl border-2 border-slate-100 bg-slate-50 peer-checked:border-primary peer-checked:bg-blue-50 transition-all">
                <span className="material-symbols-outlined text-3xl mb-2 text-slate-400 peer-checked:text-primary">deployed_code</span>
                <span className="text-xs font-medium text-slate-600 peer-checked:text-primary">Docker</span>
              </div>
            </label>
            <label className="cursor-pointer group">
              <input
                type="radio"
                name="provider_type"
                value="KUBERNETES"
                checked={providerType === 'KUBERNETES'}
                onChange={() => setProviderType('KUBERNETES')}
                className="peer sr-only"
              />
              <div className="flex flex-col items-center justify-center p-3 rounded-xl border-2 border-slate-100 bg-slate-50 peer-checked:border-primary peer-checked:bg-blue-50 transition-all">
                <span className="material-symbols-outlined text-3xl mb-2 text-slate-400 peer-checked:text-primary">hub</span>
                <span className="text-xs font-medium text-slate-600 peer-checked:text-primary">K8s</span>
              </div>
            </label>
            <label className="cursor-pointer group">
              <input
                type="radio"
                name="provider_type"
                value="AZURE_VM"
                checked={providerType === 'AZURE_VM'}
                onChange={() => setProviderType('AZURE_VM')}
                className="peer sr-only"
              />
              <div className="flex flex-col items-center justify-center p-3 rounded-xl border-2 border-slate-100 bg-slate-50 peer-checked:border-primary peer-checked:bg-blue-50 transition-all">
                <span className="material-symbols-outlined text-3xl mb-2 text-slate-400 peer-checked:text-primary">cloud</span>
                <span className="text-xs font-medium text-slate-600 peer-checked:text-primary">Azure VM</span>
              </div>
            </label>
          </div>
        </section>

        {/* Configuration Section */}
        <section className="bg-white rounded-2xl p-5 shadow-sm border border-slate-100">
          <h2 className="text-sm font-semibold text-slate-500 uppercase tracking-wider mb-4">Configuration</h2>
          <div className="space-y-4">
            <div>
              <label className="block text-sm font-medium text-slate-700 mb-1">Provider Name</label>
              <input
                type="text"
                value={name}
                onChange={(e) => setName(e.target.value)}
                className="w-full rounded-xl border-slate-200 bg-slate-50 text-slate-800 placeholder:text-slate-400 focus:border-primary focus:ring-primary sm:text-sm py-2.5 px-3"
                placeholder="e.g. build-agent-01"
              />
            </div>
            <div>
              <label className="block text-sm font-medium text-slate-700 mb-1">Endpoint URL</label>
              <div className="relative">
                <span className="absolute inset-y-0 left-0 flex items-center pl-3 text-slate-400 material-symbols-outlined text-[20px]">link</span>
                <input
                  type="url"
                  value={endpoint}
                  onChange={(e) => setEndpoint(e.target.value)}
                  className="w-full rounded-xl border-slate-200 bg-slate-50 text-slate-800 placeholder:text-slate-400 focus:border-primary focus:ring-primary sm:text-sm py-2.5 pl-10 px-3"
                  placeholder="tcp://docker:2376"
                />
              </div>
            </div>
            <div>
              <label className="block text-sm font-medium text-slate-700 mb-1">API Token / Secret</label>
              <div className="relative">
                <span className="absolute inset-y-0 left-0 flex items-center pl-3 text-slate-400 material-symbols-outlined text-[20px]">key</span>
                <input
                  type={showPassword ? 'text' : 'password'}
                  value={apiToken}
                  onChange={(e) => setApiToken(e.target.value)}
                  className="w-full rounded-xl border-slate-200 bg-slate-50 text-slate-800 placeholder:text-slate-400 focus:border-primary focus:ring-primary sm:text-sm py-2.5 pl-10 pr-10 px-3"
                  placeholder="Enter API token"
                />
                <button
                  type="button"
                  onClick={() => setShowPassword(!showPassword)}
                  className="absolute inset-y-0 right-0 flex items-center pr-3 text-slate-400 hover:text-slate-600"
                >
                  <span className="material-symbols-outlined text-[20px]">
                    {showPassword ? 'visibility' : 'visibility_off'}
                  </span>
                </button>
              </div>
            </div>
            <div>
              <label className="block text-sm font-medium text-slate-700 mb-1">Labels</label>
              <input
                type="text"
                value={labels}
                onChange={(e) => setLabels(e.target.value)}
                className="w-full rounded-xl border-slate-200 bg-slate-50 text-slate-800 placeholder:text-slate-400 focus:border-primary focus:ring-primary sm:text-sm py-2.5 px-3"
                placeholder="linux, x64, production"
              />
              <p className="mt-1 text-xs text-slate-400">Comma separated list of tags.</p>
            </div>
          </div>
        </section>

        {/* Capabilities Section */}
        <section className="bg-white rounded-2xl p-5 shadow-sm border border-slate-100">
          <h2 className="text-sm font-semibold text-slate-500 uppercase tracking-wider mb-4">Capabilities</h2>
          <div className="space-y-6">
            <div>
              <div className="flex justify-between items-center mb-2">
                <label className="text-sm font-medium text-slate-700 flex items-center gap-2">
                  <span className="material-symbols-outlined text-slate-400 text-lg">memory</span>
                  Max Memory
                </label>
                <span className="text-sm font-semibold text-primary">{maxMemory} GB</span>
              </div>
              <input
                type="range"
                min="1"
                max="64"
                value={maxMemory}
                onChange={(e) => setMaxMemory(Number(e.target.value))}
                className="w-full h-2 bg-slate-200 rounded-lg appearance-none cursor-pointer accent-primary"
              />
              <div className="flex justify-between text-xs text-slate-400 mt-1">
                <span>1GB</span>
                <span>64GB</span>
              </div>
            </div>
            <div>
              <div className="flex justify-between items-center mb-2">
                <label className="text-sm font-medium text-slate-700 flex items-center gap-2">
                  <span className="material-symbols-outlined text-slate-400 text-lg">developer_board</span>
                  Max vCPUs
                </label>
                <span className="text-sm font-semibold text-primary">{maxCpus} Cores</span>
              </div>
              <input
                type="range"
                min="1"
                max="32"
                value={maxCpus}
                onChange={(e) => setMaxCpus(Number(e.target.value))}
                className="w-full h-2 bg-slate-200 rounded-lg appearance-none cursor-pointer accent-primary"
              />
              <div className="flex justify-between text-xs text-slate-400 mt-1">
                <span>1</span>
                <span>32</span>
              </div>
            </div>
            <div className="h-px bg-slate-100 w-full my-4"></div>
            <div className="flex items-center justify-between">
              <div className="flex flex-col">
                <span className="text-sm font-medium text-slate-800">GPU Support</span>
                <span className="text-xs text-slate-500">Enable CUDA workloads</span>
              </div>
              <input
                type="checkbox"
                checked={gpuSupport}
                onChange={(e) => setGpuSupport(e.target.checked)}
                className="ios-toggle"
              />
            </div>
            <div className="flex items-center justify-between">
              <div className="flex flex-col">
                <span className="text-sm font-medium text-slate-800">Secure Boot</span>
                <span className="text-xs text-slate-500">Require signed signatures</span>
              </div>
              <input
                type="checkbox"
                checked={secureBoot}
                onChange={(e) => setSecureBoot(e.target.checked)}
                className="ios-toggle"
              />
            </div>
          </div>
        </section>

        <button className="w-full flex items-center justify-center gap-2 text-slate-500 py-3 text-sm font-medium hover:text-primary transition-colors">
          Show Advanced Options
          <span className="material-symbols-outlined text-lg">expand_more</span>
        </button>
      </main>

      {/* Bottom Action Bar */}
      <div className="fixed bottom-0 left-0 w-full bg-white border-t border-slate-200 p-4 pb-8 flex flex-col gap-3 shadow-[0_-4px_6px_-1px_rgba(0,0,0,0.05)]">
        <button
          onClick={handleSubmit}
          disabled={registerProvider.isPending}
          className="w-full bg-primary text-white font-semibold py-3.5 rounded-xl shadow-lg shadow-blue-500/30 active:scale-[0.98] transition-all flex items-center justify-center gap-2 disabled:opacity-50"
        >
          <span className="material-symbols-outlined">
            {registerProvider.isPending ? 'sync' : 'add_circle'}
          </span>
          {registerProvider.isPending ? 'Creating...' : 'Create Provider'}
        </button>
        <button
          onClick={() => navigate(-1)}
          className="w-full bg-slate-50 text-slate-600 font-medium py-3.5 rounded-xl hover:bg-slate-100 transition-colors"
        >
          Cancel
        </button>
      </div>

      <style>{`
        .ios-toggle {
          appearance: none;
          width: 3.25rem;
          height: 2rem;
          background: #e2e8f0;
          border-radius: 9999px;
          position: relative;
          cursor: pointer;
          outline: none;
          transition: background 0.2s;
        }
        .ios-toggle::after {
          content: '';
          position: absolute;
          top: 2px;
          left: 2px;
          width: calc(2rem - 4px);
          height: calc(2rem - 4px);
          background: white;
          border-radius: 50%;
          transition: transform 0.2s;
          box-shadow: 0 1px 3px 0 rgb(0 0 0 / 0.1), 0 1px 2px -1px rgb(0 0 0 / 0.1);
        }
        .ios-toggle:checked {
          background: #2b6cee;
        }
        .ios-toggle:checked::after {
          transform: translateX(1.25rem);
        }
      `}</style>
    </div>
  )
}
