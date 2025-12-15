import { describe, it, expect, vi } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { SearchInput } from '@/components/ui/SearchInput'

describe('SearchInput', () => {
  it('renders with placeholder text', () => {
    render(<SearchInput value="" onChange={() => {}} placeholder="Search Job ID or Provider..." />)
    
    expect(screen.getByPlaceholderText('Search Job ID or Provider...')).toBeInTheDocument()
  })

  it('renders search icon', () => {
    render(<SearchInput value="" onChange={() => {}} />)
    
    expect(screen.getByText('search')).toBeInTheDocument()
  })

  it('displays current value', () => {
    render(<SearchInput value="test query" onChange={() => {}} />)
    
    const input = screen.getByRole('textbox')
    expect(input).toHaveValue('test query')
  })

  it('calls onChange when typing', () => {
    const handleChange = vi.fn()
    render(<SearchInput value="" onChange={handleChange} />)
    
    const input = screen.getByRole('textbox')
    fireEvent.change(input, { target: { value: 'new value' } })
    
    expect(handleChange).toHaveBeenCalledWith('new value')
  })

  it('has dark mode styling from design', () => {
    render(<SearchInput value="" onChange={() => {}} />)
    
    const input = screen.getByRole('textbox')
    expect(input).toHaveClass('dark:bg-card-dark')
    expect(input).toHaveClass('dark:border-border-dark')
  })
})
