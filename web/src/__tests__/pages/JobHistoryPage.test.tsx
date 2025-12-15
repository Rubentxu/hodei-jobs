import { describe, it, expect } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { BrowserRouter } from 'react-router-dom'
import { JobHistoryPage } from '@/pages/JobHistoryPage'

const renderWithRouter = (ui: React.ReactElement) => {
  return render(<BrowserRouter>{ui}</BrowserRouter>)
}

describe('JobHistoryPage', () => {
  describe('Header', () => {
    it('renders Job History title', () => {
      renderWithRouter(<JobHistoryPage />)
      expect(screen.getByText('Job History')).toBeInTheDocument()
    })

    it('renders filter icon', () => {
      renderWithRouter(<JobHistoryPage />)
      expect(screen.getByText('filter_list')).toBeInTheDocument()
    })
  })

  describe('Search', () => {
    it('renders search input with placeholder', () => {
      renderWithRouter(<JobHistoryPage />)
      expect(screen.getByPlaceholderText('Search Job ID or Provider...')).toBeInTheDocument()
    })

    it('filters jobs when typing in search', () => {
      renderWithRouter(<JobHistoryPage />)
      
      const searchInput = screen.getByPlaceholderText('Search Job ID or Provider...')
      fireEvent.change(searchInput, { target: { value: '5021' } })
      
      expect(screen.getByText('Job #5021')).toBeInTheDocument()
    })
  })

  describe('Filter Chips', () => {
    it('renders all filter chips', () => {
      renderWithRouter(<JobHistoryPage />)
      
      expect(screen.getByText('All Jobs')).toBeInTheDocument()
      expect(screen.getByText('Running')).toBeInTheDocument()
      expect(screen.getByText('Failed')).toBeInTheDocument()
      expect(screen.getByText('Succeeded')).toBeInTheDocument()
      expect(screen.getByText('Pending')).toBeInTheDocument()
    })

    it('All Jobs is active by default', () => {
      renderWithRouter(<JobHistoryPage />)
      
      const allChip = screen.getByText('All Jobs').closest('button')
      expect(allChip).toHaveClass('bg-primary')
    })

    it('filters jobs when clicking Failed chip', () => {
      renderWithRouter(<JobHistoryPage />)
      
      fireEvent.click(screen.getByText('Failed'))
      
      // Should show only failed jobs
      expect(screen.getByText('Job #5020')).toBeInTheDocument()
      expect(screen.queryByText('Job #5021')).not.toBeInTheDocument() // Running job
    })
  })

  describe('Job List', () => {
    it('renders Today section', () => {
      renderWithRouter(<JobHistoryPage />)
      expect(screen.getByText('Today')).toBeInTheDocument()
    })

    it('renders Yesterday section', () => {
      renderWithRouter(<JobHistoryPage />)
      expect(screen.getByText('Yesterday')).toBeInTheDocument()
    })

    it('renders job cards', () => {
      renderWithRouter(<JobHistoryPage />)
      
      expect(screen.getByText('Job #5021')).toBeInTheDocument()
      expect(screen.getByText('AWS_Production_Build')).toBeInTheDocument()
    })

    it('renders end of history message', () => {
      renderWithRouter(<JobHistoryPage />)
      expect(screen.getByText('End of recent history')).toBeInTheDocument()
    })
  })
})
