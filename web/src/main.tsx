import React, { useEffect, useState } from 'react'
import { createRoot } from 'react-dom/client'
import { HashRouter, Routes, Route, NavLink, Navigate, useNavigate } from 'react-router-dom'
import { getToken, clearToken } from './api/client'
import { api } from './api/client'
import type { SystemInfo } from './api/types'
import Login from './pages/Login'
import Overview from './pages/Overview'
import Providers from './pages/Providers'
import Models from './pages/Models'
import VirtualModels from './pages/VirtualModels'
import Applications from './pages/Applications'
import Playground from './pages/Playground'
import Requests from './pages/Requests'
import Audit from './pages/Audit'
import Settings from './pages/Settings'
import './styles.css'

function Shell({ children }: { children: React.ReactNode }) {
  const navigate = useNavigate()
  const [info, setInfo] = useState<SystemInfo | null>(null)
  useEffect(() => {
    api.get<SystemInfo>('/api/v1/admin/config').then(setInfo).catch(() => {})
  }, [])
  const nav = [
    ['/', '总览'],
    ['/providers', 'Providers'],
    ['/models', '模型'],
    ['/virtual-models', 'Virtual Models'],
    ['/applications', '应用与 Key'],
    ['/playground', 'Playground'],
    ['/requests', '请求'],
    ['/audit', '审计'],
    ['/settings', '设置'],
  ]
  const logout = () => {
    clearToken()
    navigate('/login')
  }
  return (
    <div className="layout">
      <aside className="sidebar">
        <div className="brand">
          Enterprise AI Hub
          <small>{info ? `${info.mode} · v${info.version}` : 'AI Control Plane'}</small>
        </div>
        <nav className="nav">
          {nav.map(([to, label]) => (
            <NavLink key={to} to={to} end={to === '/'}>
              {label}
            </NavLink>
          ))}
        </nav>
        <div className="sidebar-foot">
          <a href="#" onClick={logout} style={{ color: 'inherit' }}>
            退出登录
          </a>
        </div>
      </aside>
      <main className="main">{children}</main>
    </div>
  )
}

function RequireAuth({ children }: { children: React.ReactNode }) {
  const [state, setState] = useState<'checking' | 'ok' | 'no'>(getToken() ? 'checking' : 'no')
  useEffect(() => {
    const onUnauthorized = () => setState('no')
    window.addEventListener('aihub:unauthorized', onUnauthorized)
    return () => window.removeEventListener('aihub:unauthorized', onUnauthorized)
  }, [])
  useEffect(() => {
    if (state !== 'checking') return
    api
      .get('/api/v1/admin/config')
      .then(() => setState('ok'))
      .catch(() => setState('no'))
  }, [state])
  if (state === 'ok') return <>{children}</>
  if (state === 'checking') return null
  return <Navigate to="/login" replace />
}

function App() {
  return (
    <HashRouter>
      <Routes>
        <Route path="/login" element={<Login />} />
        <Route
          path="*"
          element={
            <RequireAuth>
              <Shell>
                <Routes>
                  <Route path="/" element={<Overview />} />
                  <Route path="/providers" element={<Providers />} />
                  <Route path="/models" element={<Models />} />
                  <Route path="/virtual-models" element={<VirtualModels />} />
                  <Route path="/applications" element={<Applications />} />
                  <Route path="/playground" element={<Playground />} />
                  <Route path="/requests" element={<Requests />} />
                  <Route path="/audit" element={<Audit />} />
                  <Route path="/settings" element={<Settings />} />
                  <Route path="*" element={<Navigate to="/" replace />} />
                </Routes>
              </Shell>
            </RequireAuth>
          }
        />
      </Routes>
    </HashRouter>
  )
}

createRoot(document.getElementById('root')!).render(<App />)
