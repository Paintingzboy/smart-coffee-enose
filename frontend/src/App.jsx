import { useEffect, useState } from 'react'
import { AppProvider } from './lib/app'
import Navbar, { ROUTES } from './components/Navbar'
import { Footer, OperatorDialog, Toasts } from './components/Chrome'
import DashboardPage from './pages/DashboardPage'
import AnalyticsPage from './pages/AnalyticsPage'
import ClassificationPage from './pages/ClassificationPage'
import DevicesPage from './pages/DevicesPage'
import AboutPage from './pages/AboutPage'

const PAGES = {
  '/dashboard': DashboardPage,
  '/analytics': AnalyticsPage,
  '/classification': ClassificationPage,
  '/devices': DevicesPage,
  '/about': AboutPage,
}

function useHashRoute() {
  const read = () => {
    const p = window.location.hash.replace(/^#/, '') || '/dashboard'
    return PAGES[p] ? p : '/dashboard'
  }
  const [route, setRoute] = useState(read)
  useEffect(() => {
    const on = () => { setRoute(read()); window.scrollTo(0, 0) }
    window.addEventListener('hashchange', on)
    return () => window.removeEventListener('hashchange', on)
  }, [])
  return route
}

export default function App() {
  const route = useHashRoute()
  const Page = PAGES[route]
  useEffect(() => {
    const label = ROUTES.find((r) => r.path === route)?.label
    document.title = `${label} — Smart Coffee E-Nose`
  }, [route])
  return (
    <AppProvider>
      <div className="flex min-h-screen flex-col">
        <Navbar route={route} />
        <main className="flex-1"><Page /></main>
        <Footer />
      </div>
      <OperatorDialog />
      <Toasts />
    </AppProvider>
  )
}
