import { describe, it, expect, vi } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { FilterChips, FilterChip } from '@/components/ui/FilterChips'

const mockChips: FilterChip[] = [
  { id: 'all', label: 'All Jobs' },
  { id: 'running', label: 'Running', color: '#2b6cee' },
  { id: 'failed', label: 'Failed', color: '#ef4444' },
  { id: 'succeeded', label: 'Succeeded', color: '#10b981' },
]

describe('FilterChips', () => {
  it('renders all chips', () => {
    render(<FilterChips chips={mockChips} activeId="all" onChange={() => {}} />)
    
    expect(screen.getByText('All Jobs')).toBeInTheDocument()
    expect(screen.getByText('Running')).toBeInTheDocument()
    expect(screen.getByText('Failed')).toBeInTheDocument()
    expect(screen.getByText('Succeeded')).toBeInTheDocument()
  })

  it('highlights active chip with primary background', () => {
    render(<FilterChips chips={mockChips} activeId="running" onChange={() => {}} />)
    
    const runningChip = screen.getByText('Running').closest('button')
    expect(runningChip).toHaveClass('bg-primary')
    expect(runningChip).toHaveClass('text-white')
  })

  it('inactive chips have card background', () => {
    render(<FilterChips chips={mockChips} activeId="running" onChange={() => {}} />)
    
    const allChip = screen.getByText('All Jobs').closest('button')
    expect(allChip).toHaveClass('dark:bg-card-dark')
  })

  it('calls onChange when chip is clicked', () => {
    const handleChange = vi.fn()
    render(<FilterChips chips={mockChips} activeId="all" onChange={handleChange} />)
    
    fireEvent.click(screen.getByText('Failed'))
    expect(handleChange).toHaveBeenCalledWith('failed')
  })

  it('renders color indicator for chips with color', () => {
    const { container } = render(
      <FilterChips chips={mockChips} activeId="all" onChange={() => {}} />
    )
    
    const colorDots = container.querySelectorAll('.rounded-full.w-2.h-2')
    expect(colorDots.length).toBe(3) // Running, Failed, Succeeded have colors
  })
})
