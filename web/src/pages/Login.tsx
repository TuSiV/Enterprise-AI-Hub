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

import { t, LanguageSwitch } from '../i18n'
import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { api, setToken, setServerUrl, ApiError } from '../api/client'
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
  const [serverUrl, setServerUrlValue] = useState('')
  const [error, setError] = useState('')
  const [loading, setLoading] = useState(false)
  const navigate = useNavigate()

  const submit = async (value?: string) => {
    setLoading(true)
    setError('')
    try {
      setServerUrl(serverUrl)
      setToken(value ?? token)
      await api.get<SystemInfo>('/api/v1/admin/config')
      navigate('/')
    } catch (e) {
      setToken('')
      setError(e instanceof ApiError ? e.message : t("连接失败，请确认服务已启动"))
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
        <div className="login-language"><LanguageSwitch /></div><div className="login-emblem">AI</div>
        <h1>Enterprise AI Hub</h1>
        <p>{t("输入 Admin Token 进入控制台。本地模式下 token 存储于应用数据目录 admin_token 文件。")}</p>
        <label className="field">
          <span className="field-label">{t("服务器地址（留空使用本地工作空间）")}</span>
          <input
            value={serverUrl}
            onChange={(e) => setServerUrlValue(e.target.value)}
            placeholder="https://aihub.example.com"
          />
        </label>
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
          {loading ? t("验证中…") : t("进入控制台")}
        </button>
      </div>
    </div>
  )
}
