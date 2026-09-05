import { useEffect, useState } from 'react'
import { api } from '../api/client'
import type { VirtualModelDto, ModelDto, RouteSimulation } from '../api/types'
import { Badge, Button, Card, Field, Modal, Table, Toast, Spinner, EmptyState } from '../components/ui'

export default function VirtualModels() {
  const [vms, setVms] = useState<VirtualModelDto[]>([])
  const [models, setModels] = useState<ModelDto[]>([])
  const [loading, setLoading] = useState(true)
  const [editing, setEditing] = useState<VirtualModelDto | null>(null)
  const [creating, setCreating] = useState(false)
  const [simulation, setSimulation] = useState<RouteSimulation | null>(null)
  const [toast, setToast] = useState<{ message: string; tone: 'ok' | 'err' }>({ message: '', tone: 'ok' })

  const notify = (message: string, tone: 'ok' | 'err' = 'ok') => {
    setToast({ message, tone })
    setTimeout(() => setToast({ message: '', tone }), 2600)
  }

  const load = () =>
    Promise.all([api.get<VirtualModelDto[]>('/api/v1/admin/virtual-models'), api.get<ModelDto[]>('/api/v1/admin/models')]).then(
      ([v, m]) => {
        setVms(v)
        setModels(m)
      },
    )

  useEffect(() => {
    load().finally(() => setLoading(false))
  }, [])

  const simulate = async (vm: VirtualModelDto) => {
    try {
      setSimulation(await api.post<RouteSimulation>(`/api/v1/admin/virtual-models/${vm.id}/simulate-route`))
    } catch (e: any) {
      notify(e.message, 'err')
    }
  }

  const remove = async (vm: VirtualModelDto) => {
    if (!confirm(`删除 Virtual Model「${vm.key}」？客户端将无法继续调用该 key。`)) return
    try {
      await api.del(`/api/v1/admin/virtual-models/${vm.id}`)
      notify('已删除')
      load()
    } catch (e: any) {
      notify(e.message, 'err')
    }
  }

  if (loading) return <Spinner label="加载中…" />

  return (
    <>
      <div className="page-head">
        <div>
          <h2>Virtual Models</h2>
          <div className="page-sub">客户端只依赖 Virtual Model key；后端模型切换无需修改客户端（方案 §16.3）</div>
        </div>
        <Button variant="primary" onClick={() => setCreating(true)}>
          + 创建 Virtual Model
        </Button>
      </div>

      <Card>
        {vms.length ? (
          <Table head={['Key', '名称', '策略', 'Targets', '首选 Target', '状态', '操作']}>
            {vms.map((vm) => {
              const primary = vm.targets.find((t) => t.enabled)
              return (
                <tr key={vm.id}>
                  <td className="mono">{vm.key}</td>
                  <td>{vm.name}</td>
                  <td className="mono dim">{vm.routingStrategy}</td>
                  <td>{vm.targets.length}</td>
                  <td className="dim">{primary?.modelLabel ?? '-'}</td>
                  <td>{vm.enabled ? <Badge tone="ok">启用</Badge> : <Badge tone="muted">停用</Badge>}</td>
                  <td>
                    <div style={{ display: 'flex', gap: 4 }}>
                      <Button variant="ghost" onClick={() => setEditing(vm)}>
                        编辑
                      </Button>
                      <Button variant="ghost" onClick={() => simulate(vm)}>
                        路由模拟
                      </Button>
                      <Button variant="ghost" onClick={() => remove(vm)}>
                        删除
                      </Button>
                    </div>
                  </td>
                </tr>
              )
            })}
          </Table>
        ) : (
          <EmptyState title="暂无 Virtual Model" />
        )}
      </Card>

      {creating && (
        <VMEditor models={models} onClose={() => setCreating(false)} onSaved={() => { setCreating(false); notify('已创建'); load() }} />
      )}
      {editing && (
        <VMEditor models={models} vm={editing} onClose={() => setEditing(null)} onSaved={() => { setEditing(null); notify('已保存'); load() }} />
      )}
      {simulation && (
        <Modal title={`路由模拟：${simulation.virtualModel}`} onClose={() => setSimulation(null)} wide>
          <p className="dim" style={{ marginTop: 0 }}>
            当前选中：<b>{simulation.selected ?? '无可用 target'}</b>
          </p>
          <Table head={['顺序', 'Target', 'Priority', '状态', '排除原因']}>
            {simulation.candidates.map((c, i) => (
              <tr key={c.modelId + i}>
                <td>{i + 1}</td>
                <td>{c.label}</td>
                <td>{c.priority}</td>
                <td>{c.selected ? <Badge tone="ok">选中</Badge> : <Badge tone="muted">候选</Badge>}</td>
                <td className="dim">{c.excludedReason ?? '-'}</td>
              </tr>
            ))}
          </Table>
        </Modal>
      )}
      <Toast message={toast.message} tone={toast.tone} />
    </>
  )
}

function VMEditor({
  models,
  vm,
  onClose,
  onSaved,
}: {
  models: ModelDto[]
  vm?: VirtualModelDto
  onClose: () => void
  onSaved: () => void
}) {
  const [key, setKey] = useState(vm?.key ?? '')
  const [name, setName] = useState(vm?.name ?? '')
  const [routingStrategy, setRoutingStrategy] = useState(vm?.routingStrategy ?? 'priority_failover')
  const [retryAttempts, setRetryAttempts] = useState(String(vm?.config?.retry?.maxAttemptsPerTarget ?? 1))
  const [targets, setTargets] = useState(vm?.targets.map((t) => ({ modelId: t.modelId, priority: t.priority, enabled: t.enabled })) ?? [])
  const [error, setError] = useState('')
  const [saving, setSaving] = useState(false)

  const addTarget = () => {
    const first = models.find((m) => !targets.some((t) => t.modelId === m.id))
    if (first) setTargets([...targets, { modelId: first.id, priority: (targets.length + 1) * 10, enabled: true }])
  }

  const save = async () => {
    setSaving(true)
    setError('')
    try {
      let vmId = vm?.id
      if (vm) {
        await api.patch(`/api/v1/admin/virtual-models/${vm.id}`, { name, routingStrategy })
        await api.put(`/api/v1/admin/virtual-models/${vm.id}/targets`, {
          targets: targets.map((t, i) => ({ modelId: t.modelId, priority: t.priority, enabled: t.enabled, weight: 100 })),
        })
      } else {
        const created = await api.post<VirtualModelDto>('/api/v1/admin/virtual-models', {
          key,
          name,
          routingStrategy,
          enabled: true,
          config: { retry: { maxAttemptsPerTarget: Number(retryAttempts) || 1 } },
          targets: targets.map((t) => ({ modelId: t.modelId, priority: t.priority, enabled: t.enabled, weight: 100 })),
        })
        vmId = created.id
      }
      onSaved()
    } catch (e: any) {
      setError(e.message)
    } finally {
      setSaving(false)
    }
  }

  return (
    <Modal title={vm ? `编辑 ${vm.key}` : '创建 Virtual Model'} onClose={onClose} wide>
      {!vm && (
        <Field label="Key" hint="客户端通过此 key 调用，例如 general-smart">
          <input value={key} onChange={(e) => setKey(e.target.value)} placeholder="general-smart" />
        </Field>
      )}
      <div className="field-row">
        <Field label="名称">
          <input value={name} onChange={(e) => setName(e.target.value)} />
        </Field>
        <Field label="路由策略">
          <select value={routingStrategy} onChange={(e) => setRoutingStrategy(e.target.value)}>
            <option value="priority_failover">priority_failover（优先级 + 故障转移）</option>
            <option value="weighted" disabled>
              weighted（后续版本）
            </option>
            <option value="lowest_cost" disabled>
              lowest_cost（后续版本）
            </option>
          </select>
        </Field>
      </div>
      {!vm && (
        <Field label="每 Target 重试次数" hint="5xx/连接错误自动重试；429 默认不重试">
          <input value={retryAttempts} onChange={(e) => setRetryAttempts(e.target.value)} />
        </Field>
      )}
      <Field label={`Targets（按 priority 升序）`}>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
          {targets.map((t, i) => (
            <div key={i} style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
              <select
                value={t.modelId}
                onChange={(e) => setTargets(targets.map((x, j) => (j === i ? { ...x, modelId: e.target.value } : x)))}
                style={{ flex: 1, border: '1px solid var(--border)', borderRadius: 8, padding: '6px 8px' }}
              >
                {models.map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.displayName} · {m.providerName} / {m.modelKey}
                  </option>
                ))}
              </select>
              <input
                value={t.priority}
                onChange={(e) => setTargets(targets.map((x, j) => (j === i ? { ...x, priority: Number(e.target.value) || 0 } : x)))}
                style={{ width: 80, border: '1px solid var(--border)', borderRadius: 8, padding: '6px 8px' }}
                title="priority"
              />
              <label style={{ fontSize: 12, color: 'var(--text-dim)' }}>
                <input
                  type="checkbox"
                  checked={t.enabled}
                  onChange={(e) => setTargets(targets.map((x, j) => (j === i ? { ...x, enabled: e.target.checked } : x)))}
                />{' '}
                启用
              </label>
              <Button variant="ghost" onClick={() => setTargets(targets.filter((_, j) => j !== i))}>
                移除
              </Button>
            </div>
          ))}
          <div>
            <Button variant="ghost" onClick={addTarget}>
              + 添加 Target
            </Button>
          </div>
        </div>
      </Field>
      {error && <p style={{ color: 'var(--err)', fontSize: 12.5 }}>{error}</p>}
      <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8 }}>
        <Button onClick={onClose}>取消</Button>
        <Button variant="primary" onClick={save} disabled={saving || (!vm && !key)}>
          {saving ? '保存中…' : '保存'}
        </Button>
      </div>
    </Modal>
  )
}
