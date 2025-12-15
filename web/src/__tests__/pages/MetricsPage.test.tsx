import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import { BrowserRouter } from 'react-router-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MetricsPage } from '@/pages/MetricsPage'

const renderWithProviders = () => {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  })
  return render(
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <MetricsPage />
      </BrowserRouter>
    </QueryClientProvider>
  )
}

describe('MetricsPage', () => {
  describe('Header', () => {
    it('renders System Overview title', async () => {
      renderWithProviders()
      expect(await screen.findByText('System Overview')).toBeInTheDocument()
    })

    it('renders menu button', async () => {
      renderWithProviders()
      expect(await screen.findByText('menu')).toBeInTheDocument()
    })

    it('renders user button', async () => {
      renderWithProviders()
      expect(await screen.findByText('person')).toBeInTheDocument()
    })
  })

  describe('Time Range Selector', () => {
    it('renders all time range options', async () => {
      renderWithProviders()
      await screen.findByText('System Overview')
      
      expect(screen.getByText('1H')).toBeInTheDocument()
      expect(screen.getByText('24H')).toBeInTheDocument()
      expect(screen.getByText('7D')).toBeInTheDocument()
      expect(screen.getByText('30D')).toBeInTheDocument()
    })

    it('24H is selected by default', async () => {
      renderWithProviders()
      await screen.findByText('System Overview')
      
      const label24H = screen.getByText('24H').closest('label')
      expect(label24H).toHaveClass('bg-primary')
    })
  })

  describe('KPI Cards', () => {
    it('renders Total Jobs card', async () => {
      renderWithProviders()
      await screen.findByText('System Overview')
      
      expect(screen.getByText('Total Jobs')).toBeInTheDocument()
      expect(screen.getByText('analytics')).toBeInTheDocument()
    })

    it('renders Success Rate card', async () => {
      renderWithProviders()
      await screen.findByText('System Overview')
      
      // Multiple "Success" texts exist (KPI card + legend), check at least one
      expect(screen.getAllByText('Success').length).toBeGreaterThanOrEqual(1)
    })

    it('renders Avg Duration card', async () => {
      renderWithProviders()
      await screen.findByText('System Overview')
      
      expect(screen.getByText('Avg Duration')).toBeInTheDocument()
      expect(screen.getByText('timer')).toBeInTheDocument()
    })

    it('renders CPU Load card', async () => {
      renderWithProviders()
      await screen.findByText('System Overview')
      
      expect(screen.getByText('CPU Load')).toBeInTheDocument()
    })
  })

  describe('Job Distribution', () => {
    it('renders Job Distribution section', async () => {
      renderWithProviders()
      await screen.findByText('System Overview')
      
      expect(screen.getByText('Job Distribution')).toBeInTheDocument()
    })

    it('renders legend items', async () => {
      renderWithProviders()
      await screen.findByText('System Overview')
      
      expect(screen.getAllByText('Success').length).toBeGreaterThanOrEqual(1)
      expect(screen.getByText('Fail')).toBeInTheDocument()
      expect(screen.getByText('Pending')).toBeInTheDocument()
    })
  })

  describe('Execution Trends', () => {
    it('renders Execution Trends section', async () => {
      renderWithProviders()
      await screen.findByText('System Overview')
      
      expect(screen.getByText('Execution Trends')).toBeInTheDocument()
    })
  })

  describe('Active Providers', () => {
    it('renders Active Providers section', async () => {
      renderWithProviders()
      await screen.findByText('System Overview')
      
      expect(screen.getByText('Active Providers')).toBeInTheDocument()
    })

    it('renders provider cards with dns icon', async () => {
      renderWithProviders()
      await screen.findByText('System Overview')
      
      expect(screen.getAllByText('dns').length).toBeGreaterThanOrEqual(1)
    })
  })
})
