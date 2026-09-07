// Copyright 2026 YONGZHE CHEN
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

import { t, useLocale, LanguageSwitch } from './i18n'
import { Icon } from './components/Icon'
import React, { useEffect, useState } from 'react'
import { HashRouter, Routes, Route, NavLink, Navigate, useNavigate, useLocation } from 'react-router-dom'
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
import Prompts from './pages/Prompts'
import Knowledge from './pages/Knowledge'
import Agents from './pages/Agents'
import Evals from './pages/Evals'
import Security from './pages/Security'
import Requests from './pages/Requests'
import Audit from './pages/Audit'
import Settings from './pages/Settings'

function Shell({ children }: { children: React.ReactNode }) {
  const navigate = useNavigate()
  const location = useLocation()
  useEffect(() => { window.scrollTo(0, 0) }, [location.pathname])
  const [info, setInfo] = useState<SystemInfo | null>(null)
  useEffect(() => {
    api.get<SystemInfo>('/api/v1/admin/config').then(setInfo).catch(() => {})
  }, [])
  const [menuOpen, setMenuOpen] = useState(false)
  useEffect(() => {
    const close = (event: KeyboardEvent) => { if (event.key === 'Escape') setMenuOpen(false) }
    document.addEventListener('keydown', close)
    return () => document.removeEventListener('keydown', close)
  }, [])
  const groups = [
    { label: '工作空间', items: [['/', '总览', 'overview'], ['/models', '模型', 'models'], ['/playground', '试验场', 'chat']] },
    { label: '资源管理', items: [['/providers', '服务商', 'providers'], ['/virtual-models', '虚拟模型', 'route'], ['/applications', '应用与 Key', 'apps'], ['/prompts', '提示词', 'prompt'], ['/knowledge', '知识库', 'book'], ['/agents', '智能体', 'agent'], ['/evals', '评测', 'chart']] },
    { label: '监控与管理', items: [['/requests', '请求', 'activity'], ['/audit', '审计', 'audit'], ['/security', '安全', 'shield'], ['/settings', '设置', 'settings']] },
  ]
  const logout = () => {
    // Capture the credential before clearing local state.
    void api.post('/api/v1/auth/logout').catch(() => {})
    clearToken()
    navigate('/login')
  }
  return (
    <div className="layout">
      <a className="skip-link" href="#main-content" onClick={event => { event.preventDefault(); document.getElementById('main-content')?.focus() }}>{t('跳转到内容')}</a>
      {menuOpen && <button className="nav-backdrop" aria-label={t('关闭菜单')} onClick={() => setMenuOpen(false)} />}
      <aside className={`sidebar ${menuOpen ? 'is-open' : ''}`}>
        <div className="brand"><span className="brand-mark"><Icon name="route" /></span><div>AI Hub<small>ENTERPRISE GATEWAY</small></div></div>
        <nav className="nav" aria-label={t('主导航')}>
          {groups.map(group => <div className="nav-group" key={group.label}><div className="nav-label">{t(group.label)}</div>{group.items.map(([to, label, icon]) => <NavLink key={to} to={to} end={to === '/'} onClick={() => setMenuOpen(false)}><Icon name={icon} /><span>{t(label)}</span></NavLink>)}</div>)}
        </nav>
        <div className="sidebar-foot"><div className="workspace-indicator"><span className="status-dot" />{info ? `${info.mode} · v${info.version}` : 'AI Control Plane'}</div><button className="btn btn-ghost" onClick={logout}><Icon name="logout" />{t('退出登录')}</button></div>
      </aside>
      <div className="workspace">
        <header className="topbar"><div className="topbar-leading"><button className="btn menu-toggle" aria-label={t('切换菜单')} aria-expanded={menuOpen} onClick={() => setMenuOpen(!menuOpen)}><Icon name="menu" /></button><span className="dim">AI Hub</span><span className="topbar-divider">/</span><span>{t('控制台')}</span></div><LanguageSwitch /></header>
        <main className="main" id="main-content" tabIndex={-1}>{children}<footer className="page-footer">AI Hub <span>·</span> {t('统一接入，清晰掌控。')}</footer></main>
      </div>
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

export default function App() {
  useLocale()
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
                  <Route path="/prompts" element={<Prompts />} />
                  <Route path="/knowledge" element={<Knowledge />} />
                  <Route path="/agents" element={<Agents />} />
                  <Route path="/evals" element={<Evals />} />
                  <Route path="/security" element={<Security />} />
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


