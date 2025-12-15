import { Routes, Route } from 'react-router-dom'
import { Layout } from './components/layout/Layout'
import { DashboardPage } from './pages/DashboardPage'
import { JobHistoryPage } from './pages/JobHistoryPage'
import { JobDetailPage } from './pages/JobDetailPage'
import { NewJobPage } from './pages/NewJobPage'
import { LogStreamPage } from './pages/LogStreamPage'
import { MetricsPage } from './pages/MetricsPage'
import { ProvidersPage } from './pages/ProvidersPage'
import { ProviderDetailPage } from './pages/ProviderDetailPage'
import { NewProviderPage } from './pages/NewProviderPage'

export default function App() {
  return (
    <Routes>
      <Route path="/" element={<Layout />}>
        <Route index element={<DashboardPage />} />
        <Route path="jobs" element={<JobHistoryPage />} />
        <Route path="jobs/new" element={<NewJobPage />} />
        <Route path="jobs/:jobId" element={<JobDetailPage />} />
        <Route path="jobs/:jobId/logs" element={<LogStreamPage />} />
        <Route path="metrics" element={<MetricsPage />} />
        <Route path="providers" element={<ProvidersPage />} />
        <Route path="providers/new" element={<NewProviderPage />} />
        <Route path="providers/:providerId" element={<ProviderDetailPage />} />
      </Route>
    </Routes>
  )
}
