import { t } from '../i18n'
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
    if (tone !== 'err') setTimeout(() => setToast({ message: '', tone }), 2600)
  }

  const load = () =>
    Promise.all([api.get<VirtualModelDto[]>('/api/v1/admin/virtual-models'), api.get<ModelDto[]>('/api/v1/admin/models')]).then(
      ([v, m]) => {
        setVms(v)
        setModels(m)
      },
    )

  useEffect(() => {
    load().catch((e) => notify(e.message, 'err')).finally(() => setLoading(false))
  }, [])

  const simulate = async (vm: VirtualModelDto) => {
    try {
      setSimulation(await api.post<RouteSimulation>(`/api/v1/admin/virtual-models/${vm.id}/simulate-route`))
    } catch (e: any) {
      notify(e.message, 'err')
    }
  }

  const remove = async (vm: VirtualModelDto) => {
    if (!confirm(t("删除 Virtual Model「{0}」？客户端将无法继续调用该 key。", [vm.key]))) return
    try {
      await api.del(`/api/v1/admin/virtual-models/${vm.id}`)
      notify(t("已删除"))
      load()
    } catch (e: any) {
      notify(e.message, 'err')
    }
  }

  if (loading) return <Spinner label={t("加载中…")} />

  return (
    <>
      <div className="page-head">
        <div>
          <h2>{t('虚拟模型')}</h2>
          <div className="page-sub">{t("客户端只依赖 Virtual Model key；后端模型切换无需修改客户端")}</div>
        </div>
        <Button variant="primary" onClick={() => setCreating(true)}>
          {t("+ 创建 Virtual Model")}</Button>
      </div>

      <Card>
        {vms.length ? (
          <Table head={['Key', t("名称"), t("策略"), 'Targets', t("首选 Target"), t("状态"), t("操作")]}>
            {vms.map((vm) => {
              const primary = vm.targets.find((t) => t.enabled)
              return (
                <tr key={vm.id}>
                  <td className="mono">{vm.key}</td>
                  <td>{vm.name}</td>
                  <td className="mono dim">{vm.routingStrategy}</td>
                  <td>{vm.targets.length}</td>
                  <td className="dim">{primary?.modelLabel ?? '-'}</td>
                  <td>{vm.enabled ? <Badge tone="ok">{t("启用")}</Badge> : <Badge tone="muted">{t("停用")}</Badge>}</td>
                  <td>
                    <div style={{ display: 'flex', gap: 4 }}>
                      <Button variant="ghost" onClick={() => setEditing(vm)}>
                        {t("编辑")}</Button>
                      <Button variant="ghost" onClick={() => simulate(vm)}>
                        {t("路由模拟")}</Button>
                      <Button variant="ghost" onClick={() => remove(vm)}>
                        {t("删除")}</Button>
                    </div>
                  </td>
                </tr>
              )
            })}
          </Table>
        ) : (
          <EmptyState title={t("暂无 Virtual Model")} />
        )}
      </Card>

      {creating && (
        <VMEditor models={models} onClose={() => setCreating(false)} onSaved={() => { setCreating(false); notify(t("已创建")); load() }} />
      )}
      {editing && (
        <VMEditor models={models} vm={editing} onClose={() => setEditing(null)} onSaved={() => { setEditing(null); notify(t("已保存")); load() }} />
      )}
      {simulation && (
        <Modal title={t("路由模拟：{0}", [simulation.virtualModel])} onClose={() => setSimulation(null)} wide>
          <p className="dim" style={{ marginTop: 0 }}>
            {t("当前选中：")}<b>{simulation.selected ?? t("无可用 target")}</b>
          </p>
          <Table head={[t("顺序"), 'Target', 'Priority', t("状态"), t("排除原因")]}>
            {simulation.candidates.map((c, i) => (
              <tr key={c.modelId + i}>
                <td>{i + 1}</td>
                <td>{c.label}</td>
                <td>{c.priority}</td>
                <td>{c.selected ? <Badge tone="ok">{t("选中")}</Badge> : <Badge tone="muted">{t("候选")}</Badge>}</td>
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
  const [targets, setTargets] = useState(vm?.targets.map((target) => ({ modelId: target.modelId, priority: target.priority, enabled: target.enabled })) ?? [])
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
          targets: targets.map((target, i) => ({ modelId: target.modelId, priority: target.priority, enabled: target.enabled, weight: 100 })),
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
    <Modal title={vm ? t("编辑 {0}", [vm.key]) : t("创建 Virtual Model")} onClose={onClose} wide>
      {!vm && (
        <Field label="Key" hint={t("客户端通过此 key 调用，例如 general-smart")}>
          <input value={key} onChange={(e) => setKey(e.target.value)} placeholder="general-smart" />
        </Field>
      )}
      <div className="field-row">
        <Field label={t("名称")}>
          <input value={name} onChange={(e) => setName(e.target.value)} />
        </Field>
        <Field label={t("路由策略")}>
          <select value={routingStrategy} onChange={(e) => setRoutingStrategy(e.target.value)}>
            <option value="priority_failover">{t("priority_failover（优先级 + 故障转移）")}</option>
            <option value="weighted" disabled>
              {t("weighted（后续版本）")}</option>
            <option value="lowest_cost" disabled>
              {t("lowest_cost（后续版本）")}</option>
          </select>
        </Field>
      </div>
      {!vm && (
        <Field label={t("每 Target 重试次数")} hint={t("5xx/连接错误自动重试；429 默认不重试")}>
          <input value={retryAttempts} onChange={(e) => setRetryAttempts(e.target.value)} />
        </Field>
      )}
      <Field label={t("Targets（按 priority 升序）")}>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
          {targets.map((target, i) => (
            <div key={i} style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
              <select
                value={target.modelId}
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
                value={target.priority}
                onChange={(e) => setTargets(targets.map((x, j) => (j === i ? { ...x, priority: Number(e.target.value) || 0 } : x)))}
                style={{ width: 80, border: '1px solid var(--border)', borderRadius: 8, padding: '6px 8px' }}
                title="priority"
              />
              <label style={{ fontSize: 12, color: 'var(--text-dim)' }}>
                <input
                  type="checkbox"
                  checked={target.enabled}
                  onChange={(e) => setTargets(targets.map((x, j) => (j === i ? { ...x, enabled: e.target.checked } : x)))}
                />{' '}
                {t("启用")}</label>
              <Button variant="ghost" onClick={() => setTargets(targets.filter((_, j) => j !== i))}>
                {t("移除")}</Button>
            </div>
          ))}
          <div>
            <Button variant="ghost" onClick={addTarget}>
              {t("+ 添加 Target")}</Button>
          </div>
        </div>
      </Field>
      {error && <p style={{ color: 'var(--err)', fontSize: 12.5 }}>{error}</p>}
      <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8 }}>
        <Button onClick={onClose}>{t("取消")}</Button>
        <Button variant="primary" onClick={save} disabled={saving || (!vm && !key)}>
          {saving ? t("保存中…") : t("保存")}
        </Button>
      </div>
    </Modal>
  )
}
