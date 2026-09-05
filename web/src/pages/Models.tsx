import { useEffect, useState } from 'react'
import { api, qs } from '../api/client'
import type { ModelDto, Pricing } from '../api/types'
import { Badge, Button, Card, Field, Modal, Table, Toast, Spinner, EmptyState } from '../components/ui'

export default function Models() {
  const [models, setModels] = useState<ModelDto[]>([])
  const [providers, setProviders] = useState<{ id: string; name: string }[]>([])
  const [providerFilter, setProviderFilter] = useState('')
  const [typeFilter, setTypeFilter] = useState('')
  const [loading, setLoading] = useState(true)
  const [editing, setEditing] = useState<ModelDto | null>(null)
  const [creating, setCreating] = useState(false)
  const [toast, setToast] = useState<{ message: string; tone: 'ok' | 'err' }>({ message: '', tone: 'ok' })

  const notify = (message: string, tone: 'ok' | 'err' = 'ok') => {
    setToast({ message, tone })
    setTimeout(() => setToast({ message: '', tone }), 2600)
  }

  const load = async () => {
    const data = await api.get<ModelDto[]>(`/api/v1/admin/models${qs({ providerId: providerFilter, type: typeFilter })}`)
    setModels(data)
  }

  useEffect(() => {
    Promise.all([
      api.get<any[]>('/api/v1/admin/providers'),
      api.get<ModelDto[]>('/api/v1/admin/models'),
    ])
      .then(([p, m]) => {
        setProviders(p.map((x) => ({ id: x.id, name: x.name })))
        setModels(m)
      })
      .finally(() => setLoading(false))
  }, [])

  const reload = () => load().catch((e) => notify(e.message, 'err'))

  const toggle = async (m: ModelDto) => {
    try {
      await api.patch(`/api/v1/admin/models/${m.id}`, { enabled: !m.enabled })
      reload()
    } catch (e: any) {
      notify(e.message, 'err')
    }
  }

  const remove = async (m: ModelDto) => {
    if (!confirm(`删除模型「${m.displayName}」？`)) return
    try {
      await api.del(`/api/v1/admin/models/${m.id}`)
      notify('已删除')
      reload()
    } catch (e: any) {
      notify(e.message, 'err')
    }
  }

  if (loading) return <Spinner label="加载中…" />

  return (
    <>
      <div className="page-head">
        <div>
          <h2>模型注册表</h2>
          <div className="page-sub">物理模型与定价；Virtual Model 在此之上做路由抽象</div>
        </div>
        <Button variant="primary" onClick={() => setCreating(true)}>
          + 手工登记模型
        </Button>
      </div>

      <Card>
        <div className="filters">
          <select value={providerFilter} onChange={(e) => { setProviderFilter(e.target.value); reload() }}>
            <option value="">全部 Provider</option>
            {providers.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
          <select value={typeFilter} onChange={(e) => { setTypeFilter(e.target.value); reload() }}>
            <option value="">全部类型</option>
            {['chat', 'reasoning', 'embedding', 'rerank', 'multimodal'].map((t) => (
              <option key={t} value={t}>
                {t}
              </option>
            ))}
          </select>
        </div>
        {models.length ? (
          <Table head={['模型', 'Provider', '类型', '上下文', '定价（每 1M Tokens）', '来源', '状态', '操作']}>
            {models.map((m) => (
              <tr key={m.id}>
                <td>
                  {m.displayName}
                  <div className="dim mono">{m.modelKey}</div>
                </td>
                <td className="dim">{m.providerName}</td>
                <td>{m.modelType}</td>
                <td>{m.contextWindow ?? '-'}</td>
                <td className="mono dim">
                  in {m.pricing?.input ?? '-'} / out {m.pricing?.output ?? '-'} {m.pricing?.currency ?? ''}
                </td>
                <td>{m.discovered ? <Badge tone="info">发现</Badge> : <Badge tone="muted">手工</Badge>}</td>
                <td>{m.enabled ? <Badge tone="ok">启用</Badge> : <Badge tone="muted">停用</Badge>}</td>
                <td>
                  <div style={{ display: 'flex', gap: 4 }}>
                    <Button variant="ghost" onClick={() => setEditing(m)}>
                      编辑
                    </Button>
                    <Button variant="ghost" onClick={() => toggle(m)}>
                      {m.enabled ? '停用' : '启用'}
                    </Button>
                    <Button variant="ghost" onClick={() => remove(m)}>
                      删除
                    </Button>
                  </div>
                </td>
              </tr>
            ))}
          </Table>
        ) : (
          <EmptyState title="没有模型" hint="在 Provider 页面使用「发现模型」自动拉取，或手工登记" />
        )}
      </Card>

      {creating && (
        <ModelForm
          providers={providers}
          onClose={() => setCreating(false)}
          onSaved={() => {
            setCreating(false)
            notify('模型已登记')
            reload()
          }}
        />
      )}
      {editing && (
        <ModelForm
          providers={providers}
          model={editing}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null)
            notify('模型已保存')
            reload()
          }}
        />
      )}
      <Toast message={toast.message} tone={toast.tone} />
    </>
  )
}

function ModelForm({
  providers,
  model,
  onClose,
  onSaved,
}: {
  providers: { id: string; name: string }[]
  model?: ModelDto
  onClose: () => void
  onSaved: () => void
}) {
  const [providerId, setProviderId] = useState(model?.providerId ?? providers[0]?.id ?? '')
  const [modelKey, setModelKey] = useState(model?.modelKey ?? '')
  const [displayName, setDisplayName] = useState(model?.displayName ?? '')
  const [modelType, setModelType] = useState(model?.modelType ?? 'chat')
  const [contextWindow, setContextWindow] = useState(String(model?.contextWindow ?? ''))
  const [pricing, setPricing] = useState<Pricing>(model?.pricing ?? { currency: 'USD', unitTokens: 1000000 })
  const [error, setError] = useState('')
  const [saving, setSaving] = useState(false)

  const save = async () => {
    setSaving(true)
    setError('')
    const ctx = Number(contextWindow)
    const payload: any = {
      displayName,
      modelType,
      contextWindow: ctx > 0 ? ctx : null,
      pricing,
    }
    try {
      if (model) {
        await api.patch(`/api/v1/admin/models/${model.id}`, payload)
      } else {
        payload.providerId = providerId
        payload.modelKey = modelKey
        payload.enabled = true
        await api.post('/api/v1/admin/models', payload)
      }
      onSaved()
    } catch (e: any) {
      setError(e.message)
    } finally {
      setSaving(false)
    }
  }

  return (
    <Modal title={model ? `编辑 ${model.displayName}` : '登记模型'} onClose={onClose}>
      {!model && (
        <>
          <Field label="Provider">
            <select value={providerId} onChange={(e) => setProviderId(e.target.value)}>
              {providers.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </select>
          </Field>
          <Field label="Model Key" hint="Provider 侧真实模型名，例如 gpt-4o-mini">
            <input value={modelKey} onChange={(e) => setModelKey(e.target.value)} />
          </Field>
        </>
      )}
      <Field label="显示名称">
        <input value={displayName} onChange={(e) => setDisplayName(e.target.value)} />
      </Field>
      <div className="field-row">
        <Field label="类型">
          <select value={modelType} onChange={(e) => setModelType(e.target.value)}>
            {['chat', 'reasoning', 'embedding', 'rerank', 'multimodal'].map((t) => (
              <option key={t}>{t}</option>
            ))}
          </select>
        </Field>
        <Field label="上下文窗口">
          <input value={contextWindow} onChange={(e) => setContextWindow(e.target.value)} placeholder="如 128000" />
        </Field>
      </div>
      <div className="field-row">
        <Field label="输入价格 / 1M Tokens">
          <input value={pricing.input ?? ''} onChange={(e) => setPricing({ ...pricing, input: Number(e.target.value) || null })} />
        </Field>
        <Field label="输出价格 / 1M Tokens">
          <input value={pricing.output ?? ''} onChange={(e) => setPricing({ ...pricing, output: Number(e.target.value) || null })} />
        </Field>
      </div>
      {error && <p style={{ color: 'var(--err)', fontSize: 12.5 }}>{error}</p>}
      <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8 }}>
        <Button onClick={onClose}>取消</Button>
        <Button variant="primary" onClick={save} disabled={saving || (!model && (!modelKey || !displayName))}>
          {saving ? '保存中…' : '保存'}
        </Button>
      </div>
    </Modal>
  )
}
