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

import { t } from '../i18n'
const TOKEN_KEY = 'aihub_admin_token'
// Credentials live only in this page; remove credentials persisted by older versions.
localStorage.removeItem(TOKEN_KEY)
let activeToken: string | null = null
// Connected Desktop（§25）：workspace 支持本地与多个 Server；serverUrl 持久化
const SERVER_URL_KEY = 'aihub_server_url'

export function getServerUrl(): string {
  return localStorage.getItem(SERVER_URL_KEY) ?? ''
}

export function setServerUrl(url: string) {
  if (url.trim()) {
    const parsed = new URL(url.trim())
    if (!['http:', 'https:'].includes(parsed.protocol) || parsed.username || parsed.password || parsed.search || parsed.hash) {
      throw new TypeError('Invalid server URL')
    }
    localStorage.setItem(SERVER_URL_KEY, parsed.href.replace(/\/$/, ''))
  } else {
    localStorage.removeItem(SERVER_URL_KEY)
  }
}

export function getToken(): string | null {
  return activeToken
}

export function setToken(token: string) {
  activeToken = token || null
}

export function clearToken() {
  activeToken = null
  localStorage.removeItem(TOKEN_KEY)
}

export class ApiError extends Error {
  code: string
  status: number

  constructor(status: number, code: string, message: string) {
    super(message)
    this.status = status
    this.code = code
  }
}

/** Shared transport for JSON and streaming requests, including server selection and 401 handling. */
export async function authenticatedFetch(path: string, init?: RequestInit): Promise<Response> {
  const headers = new Headers(init?.headers)
  if (!headers.has('Content-Type')) headers.set('Content-Type', 'application/json')
  const token = getToken()
  if (token) headers.set('Authorization', `Bearer ${token}`)
  const base = getServerUrl()
  const res = await fetch(base ? `${base}${path}` : path, { ...init, headers })
  if (res.status === 401) {
    clearToken()
    window.dispatchEvent(new Event('aihub:unauthorized'))
    throw new ApiError(401, 'AIH_UNAUTHORIZED', t("未登录或 Admin Token 无效"))
  }
  return res
}

async function requestEnvelope<T = any>(path: string, init?: RequestInit): Promise<{ data: T; meta?: any }> {
  const res = await authenticatedFetch(path, init)
  const body = await res.json().catch(() => ({}))
  if (!res.ok) {
    const err = body?.error ?? {}
    throw new ApiError(res.status, err.code ?? 'UNKNOWN', err.message ?? t("请求失败 ({0})", [res.status]))
  }
  return body
}

async function request<T = any>(path: string, init?: RequestInit): Promise<T> {
  const body = await requestEnvelope<T>(path, init)
  return body?.data ?? body as T
}

export const api = {
  get: <T = any>(path: string) => request<T>(path),
  getEnvelope: <T = any>(path: string) => requestEnvelope<T>(path),
  post: <T = any>(path: string, body?: unknown) =>
    request<T>(path, { method: 'POST', body: body !== undefined ? JSON.stringify(body) : undefined }),
  put: <T = any>(path: string, body: unknown) => request<T>(path, { method: 'PUT', body: JSON.stringify(body) }),
  patch: <T = any>(path: string, body: unknown) => request<T>(path, { method: 'PATCH', body: JSON.stringify(body) }),
  del: <T = any>(path: string) => request<T>(path, { method: 'DELETE' }),
}

export function qs(params: Record<string, string | number | undefined | null>): string {
  const entries = Object.entries(params).filter(([, v]) => v !== undefined && v !== null && v !== '')
  if (!entries.length) return ''
  return '?' + entries.map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(String(v))}`).join('&')
}
