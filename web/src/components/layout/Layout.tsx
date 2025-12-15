import { Outlet } from 'react-router-dom'
import { BottomNav } from './BottomNav'

export function Layout() {
  return (
    <div className="relative flex min-h-screen w-full flex-col overflow-hidden pb-24">
      <Outlet />
      <BottomNav />
    </div>
  )
}
