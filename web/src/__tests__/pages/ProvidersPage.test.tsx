import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import { BrowserRouter } from 'react-router-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { ProvidersPage } from '@/pages/ProvidersPage'

const renderWithProviders = () => {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  })
  return render(
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <ProvidersPage />
      </BrowserRouter>
    </QueryClientProvider>
  )
}

describe('ProvidersPage', () => {
  describe('Header', () => {
    it('renders Providers title', async () => {
      renderWithProviders()
      expect(await screen.findByText('Providers')).toBeInTheDocument()
    })

    it('renders add button', async () => {
      renderWithProviders()
      expect(await screen.findByText('add')).toBeInTheDocument()
    })
  })

  describe('Search', () => {
    it('renders search input', async () => {
      renderWithProviders()
      await screen.findByText('Providers')
      
      expect(screen.getByPlaceholderText('Search providers...')).toBeInTheDocument()
    })

    it('renders search icon', async () => {
      renderWithProviders()
      await screen.findByText('Providers')
      
      expect(screen.getByText('search')).toBeInTheDocument()
    })
  })

  describe('Filter Chips', () => {
    it('renders All filter chip', async () => {
      renderWithProviders()
      await screen.findByText('Providers')
      
      expect(screen.getByRole('button', { name: /All/i })).toBeInTheDocument()
    })

    it('renders Active filter chip', async () => {
      renderWithProviders()
      await screen.findByText('Providers')
      
      expect(screen.getByRole('button', { name: /Active/i })).toBeInTheDocument()
    })

    it('renders Unhealthy filter chip', async () => {
      renderWithProviders()
      await screen.findByText('Providers')
      
      expect(screen.getByRole('button', { name: /Unhealthy/i })).toBeInTheDocument()
    })

    it('All is active by default', async () => {
      renderWithProviders()
      await screen.findByText('Providers')
      
      const allChip = screen.getByRole('button', { name: /^All$/i })
      expect(allChip).toHaveClass('bg-slate-800')
    })
  })

  describe('Provider Cards', () => {
    it('renders provider cards', async () => {
      renderWithProviders()
      await screen.findByText('Providers')
      
      // Wait for providers to load
      expect(await screen.findByText('Docker Production')).toBeInTheDocument()
    })

    it('renders provider type', async () => {
      renderWithProviders()
      await screen.findByText('Docker Production')
      
      expect(screen.getAllByText('Docker').length).toBeGreaterThanOrEqual(1)
    })

    it('renders status badges', async () => {
      renderWithProviders()
      await screen.findByText('Docker Production')
      
      // Should have Active status
      expect(screen.getAllByText('Active').length).toBeGreaterThanOrEqual(1)
    })

    it('renders chevron_right icon for navigation', async () => {
      renderWithProviders()
      await screen.findByText('Docker Production')
      
      expect(screen.getAllByText('chevron_right').length).toBeGreaterThanOrEqual(1)
    })
  })

  describe('Sort FAB', () => {
    it('renders sort floating action button', async () => {
      renderWithProviders()
      await screen.findByText('Providers')
      
      expect(screen.getByText('sort')).toBeInTheDocument()
    })
  })
})
