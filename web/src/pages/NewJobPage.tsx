import { useState } from 'react'
import { useNavigate } from 'react-router-dom'

interface EnvVar {
  key: string
  value: string
}

export function NewJobPage() {
  const navigate = useNavigate()
  const [commandType, setCommandType] = useState('shell')
  const [script, setScript] = useState('')
  const [containerImage, setContainerImage] = useState('')
  const [envVars, setEnvVars] = useState<EnvVar[]>([{ key: 'API_KEY', value: '********' }])
  const [cpuCores, setCpuCores] = useState(2)
  const [memory, setMemory] = useState(4096)
  const [storage, setStorage] = useState(1024)
  const [timeout, setTimeout] = useState(30000)
  const [gpuRequired, setGpuRequired] = useState(false)
  const [architecture, setArchitecture] = useState<'x86_64' | 'arm64'>('x86_64')
  const [provider, setProvider] = useState('')
  const [region, setRegion] = useState('')
  const [priority, setPriority] = useState<'low' | 'normal' | 'high'>('normal')
  const [allowRetry, setAllowRetry] = useState(true)

  const addEnvVar = () => {
    setEnvVars([...envVars, { key: '', value: '' }])
  }

  const removeEnvVar = (index: number) => {
    setEnvVars(envVars.filter((_, i) => i !== index))
  }

  const handleSubmit = () => {
    // TODO: Submit to API
    console.log('Submitting job...')
    navigate('/jobs')
  }

  return (
    <div className="relative flex flex-col min-h-screen w-full max-w-md mx-auto shadow-2xl bg-background-light dark:bg-background-dark border-x dark:border-slate-800">
      {/* Top App Bar */}
      <header className="sticky top-0 z-50 flex items-center justify-between px-4 py-3 bg-background-light/80 dark:bg-background-dark/80 backdrop-blur-md border-b dark:border-slate-800">
        <button
          onClick={() => navigate(-1)}
          className="flex items-center justify-center p-2 -ml-2 text-slate-500 dark:text-slate-400 hover:text-primary transition-colors rounded-full hover:bg-slate-200 dark:hover:bg-slate-800"
        >
          <span className="material-symbols-outlined text-[24px]">close</span>
        </button>
        <h1 className="text-lg font-bold tracking-tight text-center flex-1">New Job Schedule</h1>
        <button className="text-primary text-sm font-bold px-2 py-1 rounded hover:bg-primary/10 transition-colors">
          Reset
        </button>
      </header>

      {/* Main Content Form */}
      <main className="flex-1 flex flex-col gap-6 p-4 pb-32">
        {/* Core Execution Section */}
        <section className="space-y-4">
          <div className="flex items-center gap-2 pb-2 border-b dark:border-slate-800">
            <span className="material-symbols-outlined text-primary">terminal</span>
            <h3 className="text-base font-bold text-slate-800 dark:text-slate-200">Core Execution</h3>
          </div>
          <div className="space-y-4">
            {/* Command Type */}
            <div className="flex flex-col gap-2">
              <label className="text-sm font-medium text-slate-600 dark:text-slate-400">Command Type</label>
              <div className="relative">
                <select
                  value={commandType}
                  onChange={(e) => setCommandType(e.target.value)}
                  className="w-full appearance-none rounded-lg bg-white dark:bg-surface-dark border border-slate-300 dark:border-slate-700 text-slate-900 dark:text-white py-3 px-4 pr-10 focus:outline-none focus:ring-2 focus:ring-primary/50 focus:border-primary transition-all"
                >
                  <option value="shell">Shell Command</option>
                  <option value="docker">Docker Exec</option>
                  <option value="python">Python Script</option>
                  <option value="node">Node.js Script</option>
                </select>
                <div className="pointer-events-none absolute inset-y-0 right-0 flex items-center px-4 text-slate-500">
                  <span className="material-symbols-outlined">expand_more</span>
                </div>
              </div>
            </div>

            {/* Script Content */}
            <div className="flex flex-col gap-2">
              <label className="text-sm font-medium text-slate-600 dark:text-slate-400">Command / Script Content</label>
              <textarea
                value={script}
                onChange={(e) => setScript(e.target.value)}
                className="w-full rounded-lg bg-white dark:bg-surface-dark border border-slate-300 dark:border-slate-700 text-slate-900 dark:text-white p-4 font-mono text-sm focus:outline-none focus:ring-2 focus:ring-primary/50 focus:border-primary transition-all min-h-[140px] leading-relaxed"
                placeholder={`# Enter your script here\necho 'Starting job execution...'`}
              />
            </div>
          </div>
        </section>

        {/* Environment & Image */}
        <section className="space-y-4">
          <div className="flex items-center gap-2 pb-2 border-b dark:border-slate-800">
            <span className="material-symbols-outlined text-primary">layers</span>
            <h3 className="text-base font-bold text-slate-800 dark:text-slate-200">Environment &amp; Image</h3>
          </div>
          <div className="space-y-4">
            {/* Docker Image */}
            <div className="flex flex-col gap-2">
              <label className="text-sm font-medium text-slate-600 dark:text-slate-400">Container Image</label>
              <div className="relative">
                <div className="absolute inset-y-0 left-0 pl-3 flex items-center pointer-events-none">
                  <span className="material-symbols-outlined text-slate-500 text-[20px]">deployed_code</span>
                </div>
                <input
                  type="text"
                  value={containerImage}
                  onChange={(e) => setContainerImage(e.target.value)}
                  className="w-full rounded-lg bg-white dark:bg-surface-dark border border-slate-300 dark:border-slate-700 text-slate-900 dark:text-white py-3 pl-10 pr-4 focus:outline-none focus:ring-2 focus:ring-primary/50 focus:border-primary transition-all placeholder:text-slate-400 dark:placeholder:text-slate-600"
                  placeholder="e.g. ubuntu:latest"
                />
              </div>
            </div>

            {/* Env Vars */}
            <div className="flex flex-col gap-3">
              <div className="flex justify-between items-center">
                <label className="text-sm font-medium text-slate-600 dark:text-slate-400">Environment Variables</label>
                <button
                  onClick={addEnvVar}
                  className="text-xs font-semibold text-primary hover:text-primary/80 flex items-center gap-1"
                >
                  <span className="material-symbols-outlined text-[16px]">add</span> Add Variable
                </button>
              </div>
              {envVars.map((envVar, index) => (
                <div key={index} className="flex items-center gap-2">
                  <input
                    type="text"
                    value={envVar.key}
                    onChange={(e) => {
                      const newEnvVars = [...envVars]
                      newEnvVars[index].key = e.target.value
                      setEnvVars(newEnvVars)
                    }}
                    className="flex-1 min-w-0 rounded-lg bg-white dark:bg-surface-dark border border-slate-300 dark:border-slate-700 text-xs font-mono text-slate-900 dark:text-white py-2 px-3 focus:outline-none focus:ring-1 focus:ring-primary"
                    placeholder="Key"
                  />
                  <span className="text-slate-500">=</span>
                  <input
                    type="password"
                    value={envVar.value}
                    onChange={(e) => {
                      const newEnvVars = [...envVars]
                      newEnvVars[index].value = e.target.value
                      setEnvVars(newEnvVars)
                    }}
                    className="flex-1 min-w-0 rounded-lg bg-white dark:bg-surface-dark border border-slate-300 dark:border-slate-700 text-xs font-mono text-slate-900 dark:text-white py-2 px-3 focus:outline-none focus:ring-1 focus:ring-primary"
                    placeholder="Value"
                  />
                  <button
                    onClick={() => removeEnvVar(index)}
                    className="text-slate-400 hover:text-red-500 transition-colors p-1"
                  >
                    <span className="material-symbols-outlined text-[20px]">remove_circle</span>
                  </button>
                </div>
              ))}
            </div>
          </div>
        </section>

        {/* Resources Grid */}
        <section className="space-y-4">
          <div className="flex items-center gap-2 pb-2 border-b dark:border-slate-800">
            <span className="material-symbols-outlined text-primary">memory</span>
            <h3 className="text-base font-bold text-slate-800 dark:text-slate-200">Resources</h3>
          </div>
          <div className="grid grid-cols-2 gap-4">
            <div className="flex flex-col gap-1.5">
              <label className="text-xs font-medium text-slate-500 dark:text-slate-400">CPU Cores</label>
              <input
                type="number"
                value={cpuCores}
                onChange={(e) => setCpuCores(Number(e.target.value))}
                className="w-full rounded-lg bg-white dark:bg-surface-dark border border-slate-300 dark:border-slate-700 text-slate-900 dark:text-white py-2.5 px-3 focus:outline-none focus:ring-2 focus:ring-primary/50 focus:border-primary"
              />
            </div>
            <div className="flex flex-col gap-1.5">
              <label className="text-xs font-medium text-slate-500 dark:text-slate-400">Memory (MB)</label>
              <input
                type="number"
                value={memory}
                onChange={(e) => setMemory(Number(e.target.value))}
                className="w-full rounded-lg bg-white dark:bg-surface-dark border border-slate-300 dark:border-slate-700 text-slate-900 dark:text-white py-2.5 px-3 focus:outline-none focus:ring-2 focus:ring-primary/50 focus:border-primary"
              />
            </div>
            <div className="flex flex-col gap-1.5">
              <label className="text-xs font-medium text-slate-500 dark:text-slate-400">Storage (MB)</label>
              <input
                type="number"
                value={storage}
                onChange={(e) => setStorage(Number(e.target.value))}
                className="w-full rounded-lg bg-white dark:bg-surface-dark border border-slate-300 dark:border-slate-700 text-slate-900 dark:text-white py-2.5 px-3 focus:outline-none focus:ring-2 focus:ring-primary/50 focus:border-primary"
              />
            </div>
            <div className="flex flex-col gap-1.5">
              <label className="text-xs font-medium text-slate-500 dark:text-slate-400">Timeout (ms)</label>
              <input
                type="number"
                value={timeout}
                onChange={(e) => setTimeout(Number(e.target.value))}
                className="w-full rounded-lg bg-white dark:bg-surface-dark border border-slate-300 dark:border-slate-700 text-slate-900 dark:text-white py-2.5 px-3 focus:outline-none focus:ring-2 focus:ring-primary/50 focus:border-primary"
              />
            </div>
          </div>

          {/* GPU Toggle */}
          <div className="flex items-center justify-between bg-white dark:bg-surface-dark p-3 rounded-lg border border-slate-200 dark:border-slate-700">
            <div className="flex flex-col">
              <span className="text-sm font-medium text-slate-900 dark:text-white">GPU Required</span>
              <span className="text-xs text-slate-500">Enable hardware acceleration</span>
            </div>
            <input
              type="checkbox"
              checked={gpuRequired}
              onChange={(e) => setGpuRequired(e.target.checked)}
              className="ios-toggle"
            />
          </div>

          {/* Architecture */}
          <div className="flex flex-col gap-2">
            <label className="text-sm font-medium text-slate-600 dark:text-slate-400">Architecture</label>
            <div className="flex bg-white dark:bg-surface-dark p-1 rounded-lg border border-slate-200 dark:border-slate-700">
              <button
                onClick={() => setArchitecture('x86_64')}
                className={`flex-1 py-1.5 text-sm font-medium rounded transition-all ${
                  architecture === 'x86_64'
                    ? 'text-white bg-primary shadow-sm'
                    : 'text-slate-500 dark:text-slate-400 hover:text-slate-900 dark:hover:text-white'
                }`}
              >
                x86_64
              </button>
              <button
                onClick={() => setArchitecture('arm64')}
                className={`flex-1 py-1.5 text-sm font-medium rounded transition-all ${
                  architecture === 'arm64'
                    ? 'text-white bg-primary shadow-sm'
                    : 'text-slate-500 dark:text-slate-400 hover:text-slate-900 dark:hover:text-white'
                }`}
              >
                arm64
              </button>
            </div>
          </div>
        </section>

        {/* I/O & Logic */}
        <section className="space-y-4">
          <div className="flex items-center gap-2 pb-2 border-b dark:border-slate-800">
            <span className="material-symbols-outlined text-primary">hub</span>
            <h3 className="text-base font-bold text-slate-800 dark:text-slate-200">I/O &amp; Constraints</h3>
          </div>
          <div className="flex flex-col divide-y divide-slate-200 dark:divide-slate-800 border-y border-slate-200 dark:border-slate-800">
            <div className="py-3 flex justify-between items-center group cursor-pointer">
              <div className="flex items-center gap-3">
                <span className="material-symbols-outlined text-slate-400">input</span>
                <div>
                  <p className="text-sm font-medium text-slate-900 dark:text-white">Job Inputs</p>
                  <p className="text-xs text-slate-500">0 items configured</p>
                </div>
              </div>
              <span className="material-symbols-outlined text-slate-400 group-hover:text-primary transition-colors">
                chevron_right
              </span>
            </div>
            <div className="py-3 flex justify-between items-center group cursor-pointer">
              <div className="flex items-center gap-3">
                <span className="material-symbols-outlined text-slate-400">output</span>
                <div>
                  <p className="text-sm font-medium text-slate-900 dark:text-white">Job Outputs</p>
                  <p className="text-xs text-slate-500">2 items configured</p>
                </div>
              </div>
              <span className="material-symbols-outlined text-slate-400 group-hover:text-primary transition-colors">
                chevron_right
              </span>
            </div>
            <div className="py-3 flex justify-between items-center group cursor-pointer">
              <div className="flex items-center gap-3">
                <span className="material-symbols-outlined text-slate-400">rule</span>
                <div>
                  <p className="text-sm font-medium text-slate-900 dark:text-white">Placement Constraints</p>
                  <p className="text-xs text-slate-500">Optional region or node tags</p>
                </div>
              </div>
              <span className="material-symbols-outlined text-slate-400 group-hover:text-primary transition-colors">
                chevron_right
              </span>
            </div>
          </div>
        </section>

        {/* Preferences */}
        <section className="space-y-4">
          <div className="flex items-center gap-2 pb-2 border-b dark:border-slate-800">
            <span className="material-symbols-outlined text-primary">tune</span>
            <h3 className="text-base font-bold text-slate-800 dark:text-slate-200">Preferences</h3>
          </div>
          <div className="grid grid-cols-2 gap-4">
            <div className="flex flex-col gap-2">
              <label className="text-sm font-medium text-slate-600 dark:text-slate-400">Provider</label>
              <div className="relative">
                <select
                  value={provider}
                  onChange={(e) => setProvider(e.target.value)}
                  className="w-full appearance-none rounded-lg bg-white dark:bg-surface-dark border border-slate-300 dark:border-slate-700 text-slate-900 dark:text-white py-2.5 px-3 pr-8 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50 focus:border-primary transition-all"
                >
                  <option value="">Any</option>
                  <option value="aws">AWS</option>
                  <option value="gcp">GCP</option>
                </select>
                <div className="pointer-events-none absolute inset-y-0 right-0 flex items-center px-2 text-slate-500">
                  <span className="material-symbols-outlined text-[20px]">expand_more</span>
                </div>
              </div>
            </div>
            <div className="flex flex-col gap-2">
              <label className="text-sm font-medium text-slate-600 dark:text-slate-400">Region</label>
              <div className="relative">
                <select
                  value={region}
                  onChange={(e) => setRegion(e.target.value)}
                  className="w-full appearance-none rounded-lg bg-white dark:bg-surface-dark border border-slate-300 dark:border-slate-700 text-slate-900 dark:text-white py-2.5 px-3 pr-8 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50 focus:border-primary transition-all"
                >
                  <option value="">Auto</option>
                  <option value="us-east">US East</option>
                  <option value="eu-west">EU West</option>
                </select>
                <div className="pointer-events-none absolute inset-y-0 right-0 flex items-center px-2 text-slate-500">
                  <span className="material-symbols-outlined text-[20px]">expand_more</span>
                </div>
              </div>
            </div>
          </div>

          {/* Priority */}
          <div className="flex flex-col gap-2">
            <label className="text-sm font-medium text-slate-600 dark:text-slate-400">Job Priority</label>
            <div className="grid grid-cols-3 gap-2 bg-white dark:bg-surface-dark p-1.5 rounded-lg border border-slate-200 dark:border-slate-700">
              <button
                onClick={() => setPriority('low')}
                className={`flex items-center justify-center gap-1 py-2 text-xs font-medium rounded transition-all ${
                  priority === 'low'
                    ? 'text-white bg-primary shadow-sm ring-1 ring-black/5'
                    : 'text-slate-500 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-700'
                }`}
              >
                <div className="w-2 h-2 rounded-full bg-emerald-500"></div> Low
              </button>
              <button
                onClick={() => setPriority('normal')}
                className={`flex items-center justify-center gap-1 py-2 text-xs font-medium rounded transition-all ${
                  priority === 'normal'
                    ? 'text-white bg-primary shadow-sm ring-1 ring-black/5'
                    : 'text-slate-500 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-700'
                }`}
              >
                <div className={`w-2 h-2 rounded-full ${priority === 'normal' ? 'bg-white' : 'bg-slate-400'}`}></div> Normal
              </button>
              <button
                onClick={() => setPriority('high')}
                className={`flex items-center justify-center gap-1 py-2 text-xs font-medium rounded transition-all ${
                  priority === 'high'
                    ? 'text-white bg-primary shadow-sm ring-1 ring-black/5'
                    : 'text-slate-500 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-700'
                }`}
              >
                <div className="w-2 h-2 rounded-full bg-red-500"></div> High
              </button>
            </div>
          </div>

          {/* Allow Retry */}
          <div className="flex items-center justify-between pt-2">
            <div className="flex flex-col">
              <span className="text-sm font-medium text-slate-900 dark:text-white">Allow Retry</span>
              <span className="text-xs text-slate-500">Retry on failure up to 3 times</span>
            </div>
            <input
              type="checkbox"
              checked={allowRetry}
              onChange={(e) => setAllowRetry(e.target.checked)}
              className="ios-toggle"
            />
          </div>
        </section>
      </main>

      {/* Fixed Footer */}
      <footer className="fixed bottom-0 left-0 right-0 max-w-md mx-auto p-4 bg-background-light/90 dark:bg-background-dark/90 backdrop-blur-md border-t dark:border-slate-800 z-40">
        <button
          onClick={handleSubmit}
          className="w-full bg-primary hover:bg-blue-600 text-white font-bold py-3.5 px-4 rounded-xl shadow-lg shadow-blue-500/20 active:scale-[0.98] transition-all flex items-center justify-center gap-2"
        >
          <span className="material-symbols-outlined text-[20px]">schedule_send</span>
          Schedule Job
        </button>
      </footer>
    </div>
  )
}
