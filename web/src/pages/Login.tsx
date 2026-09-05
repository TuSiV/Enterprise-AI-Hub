import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { api, setToken, ApiError } from '../api/client'
import type { SystemInfo } from '../api/types'

// Tauri 桌面壳注入全局 __TAURI__（withGlobalTauri），自动完成登录
async function desktopToken(): Promise<string | null> {
  const tauri = (window as any).__TAURI__
  if (!tauri?.core?.invoke) return null
  try {
    return await tauri.core.invoke('get_admin_token')
  } catch {
    return null
  }
}

export default function Login() {
  const [token, setTokenValue] = useState('')
  const [error, setError] = useState('')
  const [loading, setLoading] = useState(false)
  const navigate = useNavigate()

  const submit = async (value?: string) => {
    setLoading(true)
    setError('')
    setToken(value ?? token)
    try {
      await api.get<SystemInfo>('/api/v1/admin/config')
      navigate('/')
    } catch (e) {
      setToken('')
      setError(e instanceof ApiError ? e.message : '连接失败，请确认服务已启动')
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    desktopToken().then((t) => {
      if (t) {
        setTokenValue(t)
        submit(t)
      }
    })
  }, [])

  return (
    <div className="login-wrap">
      <div className="login-card">
        <h1>Enterprise AI Hub</h1>
        <p>输入 Admin Token 进入控制台。本地模式下 token 存储于应用数据目录 admin_token 文件。</p>
        <label className="field">
          <span className="field-label">Admin Token</span>
          <input
            type="password"
            value={token}
            onChange={(e) => setTokenValue(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && submit()}
            placeholder="aihub admin token"
            autoFocus
          />
        </label>
        {error && <p style={{ color: 'var(--err)', fontSize: 12.5 }}>{error}</p>}
        <button className="btn btn-primary" style={{ width: '100%' }} onClick={() => submit()} disabled={loading || !token.trim()}>
          {loading ? '验证中…' : '进入控制台'}
        </button>
      </div>
    </div>
  )
}
