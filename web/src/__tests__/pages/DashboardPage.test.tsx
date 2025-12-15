import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import { BrowserRouter } from 'react-router-dom'
import { DashboardPage } from '@/pages/DashboardPage'

const renderWithRouter = (ui: React.ReactElement) => {
  return render(<BrowserRouter>{ui}</BrowserRouter>)
}

describe('DashboardPage', () => {
  describe('Header', () => {
    it('renders user avatar with initial', () => {
      renderWithRouter(<DashboardPage />)
      expect(screen.getByText('A')).toBeInTheDocument()
    })

    it('renders user info', () => {
      renderWithRouter(<DashboardPage />)
      expect(screen.getByText('Admin User')).toBeInTheDocument()
      expect(screen.getByText('Platform Admin')).toBeInTheDocument()
    })

    it('renders search and notification buttons', () => {
      renderWithRouter(<DashboardPage />)
      expect(screen.getByText('search')).toBeInTheDocument()
      expect(screen.getByText('notifications')).toBeInTheDocument()
    })
  })

  describe('Stats Cards Grid', () => {
    it('renders 4 stats cards', () => {
      renderWithRouter(<DashboardPage />)
      
      expect(screen.getByText('Total Jobs')).toBeInTheDocument()
      expect(screen.getByText('Running')).toBeInTheDocument()
      expect(screen.getByText('Failed')).toBeInTheDocument()
      expect(screen.getByText('Success')).toBeInTheDocument()
    })

    it('renders stats values', () => {
      renderWithRouter(<DashboardPage />)
      
      expect(screen.getByText('1,240')).toBeInTheDocument()
      expect(screen.getByText('12')).toBeInTheDocument()
      expect(screen.getByText('23')).toBeInTheDocument()
      expect(screen.getByText('1,205')).toBeInTheDocument()
    })

    it('renders Material Symbols icons', () => {
      renderWithRouter(<DashboardPage />)
      
      expect(screen.getByText('dataset')).toBeInTheDocument()
      // sync icon appears multiple times (stats card + job cards)
      expect(screen.getAllByText('sync').length).toBeGreaterThanOrEqual(1)
      expect(screen.getAllByText('error').length).toBeGreaterThanOrEqual(1)
      expect(screen.getAllByText('check_circle').length).toBeGreaterThanOrEqual(1)
    })
  })

  describe('System Health Card', () => {
    it('renders System Health title', () => {
      renderWithRouter(<DashboardPage />)
      expect(screen.getByText('System Health')).toBeInTheDocument()
    })

    it('renders CPU Load subtitle', () => {
      renderWithRouter(<DashboardPage />)
      expect(screen.getByText('CPU Load (24h)')).toBeInTheDocument()
    })

    it('renders uptime percentage', () => {
      renderWithRouter(<DashboardPage />)
      expect(screen.getByText('98%')).toBeInTheDocument()
    })

    it('renders online nodes indicator', () => {
      renderWithRouter(<DashboardPage />)
      expect(screen.getByText('2 Online')).toBeInTheDocument()
    })

    it('renders queue status', () => {
      renderWithRouter(<DashboardPage />)
      expect(screen.getByText('Queue: 3')).toBeInTheDocument()
    })
  })

  describe('Recent Executions', () => {
    it('renders Recent Executions header', () => {
      renderWithRouter(<DashboardPage />)
      expect(screen.getByText('Recent Executions')).toBeInTheDocument()
    })

    it('renders See All link', () => {
      renderWithRouter(<DashboardPage />)
      const seeAllLink = screen.getByText('See All')
      expect(seeAllLink).toBeInTheDocument()
      expect(seeAllLink.closest('a')).toHaveAttribute('href', '/jobs')
    })

    it('renders recent job cards', () => {
      renderWithRouter(<DashboardPage />)
      
      expect(screen.getByText('Job #1024')).toBeInTheDocument()
      expect(screen.getByText('Backend Build')).toBeInTheDocument()
      expect(screen.getByText('Job #1023')).toBeInTheDocument()
      expect(screen.getByText('Deploy Staging')).toBeInTheDocument()
    })
  })

  describe('FAB (Floating Action Button)', () => {
    it('renders FAB with add icon', () => {
      renderWithRouter(<DashboardPage />)
      expect(screen.getByText('add')).toBeInTheDocument()
    })

    it('FAB links to new job page', () => {
      renderWithRouter(<DashboardPage />)
      const addIcon = screen.getByText('add')
      const fabLink = addIcon.closest('a')
      expect(fabLink).toHaveAttribute('href', '/jobs/new')
    })
  })
})
