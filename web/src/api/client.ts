const TOKEN_KEY = 'aihub_admin_token'

export function getToken(): string | null {
  return localStorage.getItem(TOKEN_KEY)
}

export function setToken(token: string) {
  localStorage.setItem(TOKEN_KEY, token)
}

export function clearToken() {
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

async function request<T = any>(path: string, init?: RequestInit): Promise<T> {
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    ...(init?.headers as Record<string, string>),
  }
  const token = getToken()
  if (token) headers['Authorization'] = `Bearer ${token}`

  const res = await fetch(path, { ...init, headers })
  if (res.status === 401) {
    clearToken()
    window.dispatchEvent(new Event('aihub:unauthorized'))
    throw new ApiError(401, 'AIH_UNAUTHORIZED', '未登录或 Admin Token 无效')
  }
  const body = await res.json().catch(() => ({}))
  if (!res.ok) {
    const err = body?.error ?? {}
    throw new ApiError(res.status, err.code ?? 'UNKNOWN', err.message ?? `请求失败 (${res.status})`)
  }
  return body?.data ?? body
}

/** 返回完整 {data, meta} 信封（分页接口需要 meta.total）。 */
async function requestEnvelope<T = any>(path: string, init?: RequestInit): Promise<{ data: T; meta?: any }> {
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    ...(init?.headers as Record<string, string>),
  }
  const token = getToken()
  if (token) headers['Authorization'] = `Bearer ${token}`
  const res = await fetch(path, { ...init, headers })
  if (res.status === 401) {
    clearToken()
    window.dispatchEvent(new Event('aihub:unauthorized'))
    throw new ApiError(401, 'AIH_UNAUTHORIZED', '未登录或 Admin Token 无效')
  }
  const body = await res.json().catch(() => ({}))
  if (!res.ok) {
    const err = body?.error ?? {}
    throw new ApiError(res.status, err.code ?? 'UNKNOWN', err.message ?? `请求失败 (${res.status})`)
  }
  return body
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
