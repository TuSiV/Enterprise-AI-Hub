import { useEffect, useState } from 'react'
import { api, qs } from '../api/client'
import type { ProviderDto, ModelDto } from '../api/types'
import { formatTime } from '../api/types'
import { Button, Card, Field, Modal, Table, HealthBadge, Toast, Spinner, EmptyState, Badge } from '../components/ui'

const KINDS = [
  ['openai_compatible', 'OpenAI Compatible'],
  ['openai', 'OpenAI'],
  ['ollama', 'Ollama'],
  ['anthropic', 'Anthropic（Adapter 待接入）'],
  ['gemini', 'Gemini（Adapter 待接入）'],
]

export default function Providers() {
  const [providers, setProviders] = useState<ProviderDto[]>([])
  const [loading, setLoading] = useState(true)
  const [editing, setEditing] = useState<ProviderDto | null>(null)
  const [creating, setCreating] = useState(false)
  const [toast, setToast] = useState<{ message: string; tone: 'ok' | 'err' }>({ message: '', tone: 'ok' })
  const [discovered, setDiscovered] = useState<ModelDto[] | null>(null)

  const notify = (message: string, tone: 'ok' | 'err' = 'ok') => {
    setToast({ message, tone })
    setTimeout(() => setToast({ message: '', tone }), 2600)
  }

  const load = () => api.get<ProviderDto[]>('/api/v1/admin/providers').then(setProviders)

  useEffect(() => {
    load().finally(() => setLoading(false))
  }, [])

  const toggle = async (p: ProviderDto) => {
    try {
      await api.patch(`/api/v1/admin/providers/${p.id}`, { enabled: !p.enabled })
      await load()
    } catch (e: any) {
      notify(e.message, 'err')
    }
  }

  const test = async (p: ProviderDto) => {
    try {
      const r = await api.post<{ ok: boolean; latencyMs: number | null; error: string | null }>(
        `/api/v1/admin/providers/${p.id}/test`,
      )
      notify(r.ok ? `连接正常（${r.latencyMs}ms）` : `失败：${r.error ?? 'unknown'}`, r.ok ? 'ok' : 'err')
      await load()
    } catch (e: any) {
      notify(e.message, 'err')
    }
  }

  const discover = async (p: ProviderDto) => {
    try {
      const r = await api.post<{ discovered: number; created: number; updated: number; models: ModelDto[] }>(
        `/api/v1/admin/providers/${p.id}/discover-models`,
      )
      notify(`发现 ${r.discovered} 个模型（新增 ${r.created} / 更新 ${r.updated}）`)
      setDiscovered(r.models)
      await load()
    } catch (e: any) {
      notify(e.message, 'err')
    }
  }

  const remove = async (p: ProviderDto) => {
    if (!confirm(`删除 Provider「${p.name}」？`)) return
    try {
      await api.del(`/api/v1/admin/providers/${p.id}`)
      notify('已删除')
      await load()
    } catch (e: any) {
      notify(e.message, 'err')
    }
  }

  if (loading) return <Spinner label="加载中…" />

  return (
    <>
      <div className="page-head">
        <div>
          <h2>Providers</h2>
          <div className="page-sub">统一接入模型提供方；凭据保存在系统 Secret Store（macOS Keychain）</div>
        </div>
        <Button variant="primary" onClick={() => setCreating(true)}>
          + 添加 Provider
        </Button>
      </div>

      <Card>
        {providers.length ? (
          <Table head={['名称', '类型', 'Base URL', '模型', '健康', '最近检测', '状态', '操作']}>
            {providers.map((p) => (
              <tr key={p.id}>
                <td>
                  {p.name}
                  <div className="dim mono">{p.key}</div>
                </td>
                <td className="mono">{p.kind}</td>
                <td className="mono dim">{p.baseUrl}</td>
                <td>{p.modelCount}</td>
                <td>
                  <HealthBadge health={p.health} />
                </td>
                <td className="dim">{formatTime(p.lastHealthCheckAt)}</td>
                <td>{p.enabled ? <Badge tone="ok">启用</Badge> : <Badge tone="muted">停用</Badge>}</td>
                <td>
                  <div style={{ display: 'flex', gap: 4, flexWrap: 'wrap' }}>
                    <Button variant="ghost" onClick={() => test(p)}>
                      测试
                    </Button>
                    <Button variant="ghost" onClick={() => discover(p)}>
                      发现模型
                    </Button>
                    <Button variant="ghost" onClick={() => setEditing(p)}>
                      编辑
                    </Button>
                    <Button variant="ghost" onClick={() => toggle(p)}>
                      {p.enabled ? '停用' : '启用'}
                    </Button>
                    <Button variant="ghost" onClick={() => remove(p)}>
                      删除
                    </Button>
                  </div>
                </td>
              </tr>
            ))}
          </Table>
        ) : (
          <EmptyState title="还没有 Provider" hint="添加一个 OpenAI 兼容 Provider 开始使用" />
        )}
      </Card>

      {creating && (
        <ProviderForm
          title="添加 Provider"
          onClose={() => setCreating(false)}
          onSaved={(p) => {
            setCreating(false)
            notify(`Provider「${p.name}」已创建`)
            load()
          }}
        />
      )}
      {editing && (
        <ProviderForm
          title={`编辑 ${editing.name}`}
          provider={editing}
          onClose={() => setEditing(null)}
          onSaved={(p) => {
            setEditing(null)
            notify(`Provider「${p.name}」已保存`)
            load()
          }}
        />
      )}
      {discovered && (
        <Modal title="发现的模型" onClose={() => setDiscovered(null)} wide>
          <Table head={['Model Key', '类型', '状态']}>
            {discovered.map((m) => (
              <tr key={m.id}>
                <td className="mono">{m.modelKey}</td>
                <td>{m.modelType}</td>
                <td>{m.enabled ? <Badge tone="ok">启用</Badge> : <Badge tone="muted">停用</Badge>}</td>
              </tr>
            ))}
          </Table>
        </Modal>
      )}
      <Toast message={toast.message} tone={toast.tone} />
    </>
  )
}

function ProviderForm({
  title,
  provider,
  onClose,
  onSaved,
}: {
  title: string
  provider?: ProviderDto
  onClose: () => void
  onSaved: (p: ProviderDto) => void
}) {
  const [key, setKey] = useState(provider?.key ?? '')
  const [name, setName] = useState(provider?.name ?? '')
  const [kind, setKind] = useState(provider?.kind ?? 'openai_compatible')
  const [baseUrl, setBaseUrl] = useState(provider?.baseUrl ?? '')
  const [apiKey, setApiKey] = useState('')
  const [timeoutMs, setTimeoutMs] = useState(String(provider?.timeoutMs ?? 120000))
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState('')

  const save = async () => {
    setSaving(true)
    setError('')
    try {
      const payload: any = {
        name,
        kind,
        baseUrl,
        timeoutMs: Number(timeoutMs) || 120000,
      }
      if (apiKey) payload.apiKey = apiKey
      let saved: ProviderDto
      if (provider) {
        saved = await api.patch<ProviderDto>(`/api/v1/admin/providers/${provider.id}`, payload)
      } else {
        payload.key = key
        payload.enabled = true
        saved = await api.post<ProviderDto>('/api/v1/admin/providers', payload)
      }
      onSaved(saved)
    } catch (e: any) {
      setError(e.message)
    } finally {
      setSaving(false)
    }
  }

  return (
    <Modal title={title} onClose={onClose}>
      {!provider && (
        <Field label="Key（稳定标识）" hint="用于程序引用，例如 openai-primary">
          <input value={key} onChange={(e) => setKey(e.target.value)} placeholder="openai-primary" />
        </Field>
      )}
      <Field label="名称">
        <input value={name} onChange={(e) => setName(e.target.value)} placeholder="显示名称" />
      </Field>
      <Field label="类型">
        <select value={kind} onChange={(e) => setKind(e.target.value)}>
          {KINDS.map(([v, l]) => (
            <option key={v} value={v}>
              {l}
            </option>
          ))}
        </select>
      </Field>
      <Field label="Base URL" hint="例如 https://api.openai.com/v1 或 http://127.0.0.1:11434/v1">
        <input value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)} placeholder="https://…/v1" />
      </Field>
      <Field label={provider ? 'API Key（留空保持不变，填写即轮换）' : 'API Key'}>
        <input type="password" value={apiKey} onChange={(e) => setApiKey(e.target.value)} placeholder={provider?.credentialConfigured ? '已配置' : 'sk-…'} />
      </Field>
      <Field label="超时（毫秒）">
        <input value={timeoutMs} onChange={(e) => setTimeoutMs(e.target.value)} />
      </Field>
      {error && <p style={{ color: 'var(--err)', fontSize: 12.5 }}>{error}</p>}
      <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8 }}>
        <Button onClick={onClose}>取消</Button>
        <Button variant="primary" onClick={save} disabled={saving || !name || !baseUrl}>
          {saving ? '保存中…' : '保存'}
        </Button>
      </div>
    </Modal>
  )
}
