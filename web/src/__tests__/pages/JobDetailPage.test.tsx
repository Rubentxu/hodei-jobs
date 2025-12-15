import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import { BrowserRouter, Routes, Route } from 'react-router-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { JobDetailPage } from '@/pages/JobDetailPage'

const renderWithRouter = (jobId: string = '5021') => {
  window.history.pushState({}, '', `/jobs/${jobId}`)
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  })
  return render(
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <Routes>
          <Route path="/jobs/:jobId" element={<JobDetailPage />} />
        </Routes>
      </BrowserRouter>
    </QueryClientProvider>
  )
}

describe('JobDetailPage', () => {
  describe('Header', () => {
    it('renders Job Details title', async () => {
      renderWithRouter('5021')
      expect(await screen.findByText('Job Details')).toBeInTheDocument()
    })

    it('renders back button', async () => {
      renderWithRouter('5021')
      expect(await screen.findByText('arrow_back')).toBeInTheDocument()
    })

    it('renders more options button', async () => {
      renderWithRouter('5021')
      expect(await screen.findByText('more_vert')).toBeInTheDocument()
    })
  })

  describe('Job Header Section', () => {
    it('renders job ID', async () => {
      renderWithRouter('5021')
      expect(await screen.findByText(/#JOB-5021/)).toBeInTheDocument()
    })

    it('renders status badge', async () => {
      renderWithRouter('5021')
      // Wait for data to load
      await screen.findByText(/#JOB-5021/)
      // Status should be displayed - multiple elements may match
      expect(screen.getAllByText(/Running|RUNNING/i).length).toBeGreaterThanOrEqual(1)
    })

    it('renders metadata chips', async () => {
      renderWithRouter('5021')
      await screen.findByText(/#JOB-5021/)
      expect(screen.getByText('DataPipeline')).toBeInTheDocument()
    })
  })

  describe('Tabs', () => {
    it('renders all 4 tabs', async () => {
      renderWithRouter('5021')
      await screen.findByText(/#JOB-5021/)
      
      expect(screen.getByText('Overview')).toBeInTheDocument()
      expect(screen.getByText('Config')).toBeInTheDocument()
      expect(screen.getByText('Logs')).toBeInTheDocument()
      expect(screen.getByText('Res')).toBeInTheDocument()
    })

    it('Overview tab is active by default', async () => {
      renderWithRouter('5021')
      await screen.findByText(/#JOB-5021/)
      
      // Timeline section should be visible (Overview content)
      expect(screen.getByText('Timeline')).toBeInTheDocument()
    })
  })

  describe('Timeline Section', () => {
    it('renders timeline steps', async () => {
      renderWithRouter('5021')
      await screen.findByText(/#JOB-5021/)
      
      expect(screen.getByText('Job Queued')).toBeInTheDocument()
      expect(screen.getByText('Image Pulled')).toBeInTheDocument()
      expect(screen.getByText('Running Script')).toBeInTheDocument()
      expect(screen.getByText('Cleanup')).toBeInTheDocument()
    })
  })

  describe('Resource Cards', () => {
    it('renders CPU Usage card', async () => {
      renderWithRouter('5021')
      await screen.findByText(/#JOB-5021/)
      
      expect(screen.getByText('CPU Usage')).toBeInTheDocument()
      expect(screen.getByText('memory')).toBeInTheDocument()
    })

    it('renders Memory card', async () => {
      renderWithRouter('5021')
      await screen.findByText(/#JOB-5021/)
      
      expect(screen.getByText('Memory')).toBeInTheDocument()
      expect(screen.getByText('database')).toBeInTheDocument()
    })
  })

  describe('Bottom Action Bar', () => {
    it('renders SSH Access button', async () => {
      renderWithRouter('5021')
      await screen.findByText(/#JOB-5021/)
      
      expect(screen.getByText('SSH Access')).toBeInTheDocument()
      expect(screen.getByText('terminal')).toBeInTheDocument()
    })

    it('renders Cancel Job button', async () => {
      renderWithRouter('5021')
      await screen.findByText(/#JOB-5021/)
      
      expect(screen.getByText('Cancel Job')).toBeInTheDocument()
      expect(screen.getByText('cancel')).toBeInTheDocument()
    })
  })
})
