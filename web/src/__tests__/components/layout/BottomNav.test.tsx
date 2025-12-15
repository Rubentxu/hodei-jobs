import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import { BrowserRouter } from 'react-router-dom'
import { BottomNav } from '@/components/layout/BottomNav'

const renderWithRouter = (ui: React.ReactElement) => {
  return render(<BrowserRouter>{ui}</BrowserRouter>)
}

describe('BottomNav', () => {
  it('renders 4 navigation items', () => {
    renderWithRouter(<BottomNav />)
    
    const navLinks = screen.getAllByRole('link')
    expect(navLinks).toHaveLength(4)
  })

  it('renders correct labels: Overview, Jobs, Providers, Metrics', () => {
    renderWithRouter(<BottomNav />)
    
    expect(screen.getByText('Overview')).toBeInTheDocument()
    expect(screen.getByText('Jobs')).toBeInTheDocument()
    expect(screen.getByText('Providers')).toBeInTheDocument()
    expect(screen.getByText('Metrics')).toBeInTheDocument()
  })

  it('renders Material Symbols icons', () => {
    renderWithRouter(<BottomNav />)
    
    expect(screen.getByText('dashboard')).toBeInTheDocument()
    expect(screen.getByText('history')).toBeInTheDocument()
    expect(screen.getByText('dns')).toBeInTheDocument()
    expect(screen.getByText('monitoring')).toBeInTheDocument()
  })

  it('has correct navigation links', () => {
    renderWithRouter(<BottomNav />)
    
    const links = screen.getAllByRole('link')
    expect(links[0]).toHaveAttribute('href', '/')
    expect(links[1]).toHaveAttribute('href', '/jobs')
    expect(links[2]).toHaveAttribute('href', '/providers')
    expect(links[3]).toHaveAttribute('href', '/metrics')
  })

  it('applies dark mode classes from design', () => {
    renderWithRouter(<BottomNav />)
    
    const nav = screen.getAllByRole('link')[0].parentElement
    expect(nav).toHaveClass('dark:bg-[#161b26]')
  })
})
