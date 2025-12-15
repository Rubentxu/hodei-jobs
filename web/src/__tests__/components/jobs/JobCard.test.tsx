import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import { BrowserRouter } from 'react-router-dom'
import { JobCard } from '@/components/jobs/JobCard'

const renderWithRouter = (ui: React.ReactElement) => {
  return render(<BrowserRouter>{ui}</BrowserRouter>)
}

describe('JobCard', () => {
  it('renders job ID and name', () => {
    renderWithRouter(
      <JobCard
        id="5021"
        name="AWS_Production_Build"
        status="RUNNING"
        startedAt="10:05 AM"
        duration="2m 15s"
      />
    )
    
    expect(screen.getByText('Job #5021')).toBeInTheDocument()
    expect(screen.getByText('AWS_Production_Build')).toBeInTheDocument()
  })

  it('renders time and duration', () => {
    renderWithRouter(
      <JobCard
        id="5021"
        name="Test Job"
        status="SUCCEEDED"
        startedAt="09:15 AM"
        duration="12m 30s"
      />
    )
    
    expect(screen.getByText('09:15 AM')).toBeInTheDocument()
    expect(screen.getByText('12m 30s')).toBeInTheDocument()
  })

  it('shows Running status with sync icon', () => {
    renderWithRouter(
      <JobCard
        id="5021"
        name="Test"
        status="RUNNING"
        startedAt="10:00"
        duration="1m"
      />
    )
    
    expect(screen.getByText('Running')).toBeInTheDocument()
    expect(screen.getByText('sync')).toBeInTheDocument()
  })

  it('shows Failed status with error icon', () => {
    renderWithRouter(
      <JobCard
        id="5020"
        name="Failed Job"
        status="FAILED"
        startedAt="09:45 AM"
        duration="45s"
      />
    )
    
    expect(screen.getByText('Failed')).toBeInTheDocument()
    expect(screen.getByText('error')).toBeInTheDocument()
  })

  it('shows Succeeded status with check_circle icon', () => {
    renderWithRouter(
      <JobCard
        id="5019"
        name="Success Job"
        status="SUCCEEDED"
        startedAt="09:15 AM"
        duration="12m"
      />
    )
    
    expect(screen.getByText('Succeeded')).toBeInTheDocument()
    expect(screen.getByText('check_circle')).toBeInTheDocument()
  })

  it('shows Pending status with schedule icon', () => {
    renderWithRouter(
      <JobCard
        id="5018"
        name="Pending Job"
        status="PENDING"
        startedAt="11:15 PM"
        duration="-"
      />
    )
    
    expect(screen.getByText('Pending')).toBeInTheDocument()
    expect(screen.getByText('schedule')).toBeInTheDocument()
  })

  it('renders progress bar for RUNNING jobs', () => {
    const { container } = renderWithRouter(
      <JobCard
        id="5021"
        name="Running Job"
        status="RUNNING"
        startedAt="10:00"
        duration="2m"
        progress={45}
      />
    )
    
    const progressBar = container.querySelector('.bg-primary')
    expect(progressBar).toBeInTheDocument()
    expect(progressBar).toHaveStyle({ width: '45%' })
  })

  it('links to job detail page', () => {
    renderWithRouter(
      <JobCard
        id="5021"
        name="Test"
        status="RUNNING"
        startedAt="10:00"
        duration="1m"
      />
    )
    
    const link = screen.getByRole('link')
    expect(link).toHaveAttribute('href', '/jobs/5021')
  })

  it('has touch feedback class from design', () => {
    renderWithRouter(
      <JobCard
        id="5021"
        name="Test"
        status="RUNNING"
        startedAt="10:00"
        duration="1m"
      />
    )
    
    const link = screen.getByRole('link')
    expect(link).toHaveClass('active:scale-[0.99]')
  })

  it('applies opacity for old jobs', () => {
    renderWithRouter(
      <JobCard
        id="5018"
        name="Old Job"
        status="SUCCEEDED"
        startedAt="Yesterday"
        duration="45m"
        isOld
      />
    )
    
    const link = screen.getByRole('link')
    expect(link).toHaveClass('opacity-80')
  })
})
