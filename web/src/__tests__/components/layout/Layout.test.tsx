import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import { BrowserRouter, Routes, Route } from 'react-router-dom'
import { Layout } from '@/components/layout/Layout'

const renderWithRouter = (ui: React.ReactElement, { route = '/' } = {}) => {
  window.history.pushState({}, 'Test page', route)
  return render(<BrowserRouter>{ui}</BrowserRouter>)
}

describe('Layout', () => {
  it('renders BottomNav', () => {
    renderWithRouter(
      <Routes>
        <Route path="/" element={<Layout />}>
          <Route index element={<div>Home Content</div>} />
        </Route>
      </Routes>
    )
    
    expect(screen.getByText('Overview')).toBeInTheDocument()
    expect(screen.getByText('Jobs')).toBeInTheDocument()
  })

  it('renders child content via Outlet', () => {
    renderWithRouter(
      <Routes>
        <Route path="/" element={<Layout />}>
          <Route index element={<div data-testid="child-content">Test Child</div>} />
        </Route>
      </Routes>
    )
    
    expect(screen.getByTestId('child-content')).toBeInTheDocument()
    expect(screen.getByText('Test Child')).toBeInTheDocument()
  })

  it('has padding bottom for BottomNav safe area', () => {
    const { container } = renderWithRouter(
      <Routes>
        <Route path="/" element={<Layout />}>
          <Route index element={<div>Content</div>} />
        </Route>
      </Routes>
    )
    
    const layoutDiv = container.firstChild as HTMLElement
    expect(layoutDiv).toHaveClass('pb-24')
  })
})
