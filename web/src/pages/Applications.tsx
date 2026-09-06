import { t } from '../i18n'
import { useEffect, useState } from 'react'
import { api } from '../api/client'
import type { ApplicationDto, ApiKeyDto, VirtualModelDto, QuotaInput } from '../api/types'
import { formatCost, formatTime } from '../api/types'
import { Badge, Button, Card, Field, Modal, Table, Toast, Spinner, EmptyState } from '../components/ui'

export default function Applications() {
  const [apps, setApps] = useState<ApplicationDto[]>([])
  const [vms, setVms] = useState<VirtualModelDto[]>([])
  const [loading, setLoading] = useState(true)
  const [editing, setEditing] = useState<ApplicationDto | null>(null)
  const [creating, setCreating] = useState(false)
  const [keysOf, setKeysOf] = useState<ApplicationDto | null>(null)
  const [toast, setToast] = useState<{ message: string; tone: 'ok' | 'err' }>({ message: '', tone: 'ok' })

  const notify = (message: string, tone: 'ok' | 'err' = 'ok') => {
    setToast({ message, tone })
    if (tone !== 'err') setTimeout(() => setToast({ message: '', tone }), 2600)
  }

  const load = () =>
    Promise.all([api.get<ApplicationDto[]>('/api/v1/admin/applications'), api.get<VirtualModelDto[]>('/api/v1/admin/virtual-models')]).then(
      ([a, v]) => {
        setApps(a)
        setVms(v)
      },
    )

  useEffect(() => {
    load().catch((e) => notify(e.message, 'err')).finally(() => setLoading(false))
  }, [])

  const remove = async (app: ApplicationDto) => {
    if (!confirm(t("删除应用「{0}」及其全部 API Key？", [app.name]))) return
    try {
      await api.del(`/api/v1/admin/applications/${app.id}`)
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
          <h2>{t("应用与 API Key")}</h2>
          <div className="page-sub">{t("业务系统通过 Application API Key 调用 Gateway；支持模型白名单与配额")}</div>
        </div>
        <Button variant="primary" onClick={() => setCreating(true)}>
          {t("+ 创建应用")}</Button>
      </div>

      <Card>
        {apps.length ? (
          <Table head={[t("应用"), 'Key', t("允许的 Virtual Models"), t("直连模型"), t("Key 数"), t("状态"), t("操作")]}>
            {apps.map((app) => (
              <tr key={app.id}>
                <td>
                  {app.name}
                  <div className="dim mono">{app.key}</div>
                </td>
                <td className="mono dim">{app.key}</td>
                <td className="dim">
                  {app.allowedVirtualModels.length ? app.allowedVirtualModels.map((k) => <Badge key={k}>{k}</Badge>) : t("全部")}
                </td>
                <td>{app.allowDirectModels ? <Badge tone="warn">{t("允许")}</Badge> : <Badge tone="muted">{t("禁止")}</Badge>}</td>
                <td>{app.keyCount}</td>
                <td>
                  <StatusBadge status={app.status} />
                </td>
                <td>
                  <div style={{ display: 'flex', gap: 4 }}>
                    <Button variant="ghost" onClick={() => setKeysOf(app)}>
                      API Keys
                    </Button>
                    <Button variant="ghost" onClick={() => setEditing(app)}>
                      {t("编辑")}</Button>
                    <Button variant="ghost" onClick={() => remove(app)}>
                      {t("删除")}</Button>
                  </div>
                </td>
              </tr>
            ))}
          </Table>
        ) : (
          <EmptyState title={t("暂无应用")} />
        )}
      </Card>

      {creating && <AppForm vms={vms} onClose={() => setCreating(false)} onSaved={() => { setCreating(false); notify(t("已创建")); load() }} />}
      {editing && <AppForm vms={vms} app={editing} onClose={() => setEditing(null)} onSaved={() => { setEditing(null); notify(t("已保存")); load() }} />}
      {keysOf && <KeysPanel app={keysOf} onClose={() => setKeysOf(null)} onChanged={load} />}
      <Toast message={toast.message} tone={toast.tone} />
    </>
  )
}

function StatusBadge({ status }: { status: string }) {
  return status === 'active' ? <Badge tone="ok">{t("启用")}</Badge> : <Badge tone="muted">{status}</Badge>
}

function AppForm({
  vms,
  app,
  onClose,
  onSaved,
}: {
  vms: VirtualModelDto[]
  app?: ApplicationDto
  onClose: () => void
  onSaved: () => void
}) {
  const [key, setKey] = useState(app?.key ?? '')
  const [name, setName] = useState(app?.name ?? '')
  const [allowed, setAllowed] = useState<string[]>(app?.allowedVirtualModels ?? [])
  const [allowDirect, setAllowDirect] = useState(app?.allowDirectModels ?? false)
  const [budget, setBudget] = useState(app?.monthlyBudgetMicrounits ? String(app.monthlyBudgetMicrounits) : '')
  const [rpm, setRpm] = useState(app?.quota?.rpm ? String(app.quota.rpm) : '')
  const [monthlyCost, setMonthlyCost] = useState(app?.quota?.monthlyCostMicrounits ? String(app.quota.monthlyCostMicrounits) : '')
  const [error, setError] = useState('')
  const [saving, setSaving] = useState(false)

  const toggleVm = (k: string) => {
    setAllowed(allowed.includes(k) ? allowed.filter((x) => x !== k) : [...allowed, k])
  }

  const save = async () => {
    setSaving(true)
    setError('')
    const quota: QuotaInput = {
      rpm: rpm ? Number(rpm) : null,
      monthlyCostMicrounits: monthlyCost ? Number(monthlyCost) : null,
      exceedAction: 'block',
    }
    try {
      if (app) {
        await api.patch(`/api/v1/admin/applications/${app.id}`, {
          name,
          allowedVirtualModels: allowed,
          allowDirectModels: allowDirect,
          monthlyBudgetMicrounits: budget ? Number(budget) : null,
          quota,
        })
      } else {
        await api.post('/api/v1/admin/applications', {
          key,
          name,
          allowedVirtualModels: allowed,
          allowDirectModels: allowDirect,
          monthlyBudgetMicrounits: budget ? Number(budget) : null,
          quota,
        })
      }
      onSaved()
    } catch (e: any) {
      setError(e.message)
    } finally {
      setSaving(false)
    }
  }

  return (
    <Modal title={app ? t("编辑 {0}", [app.name]) : t("创建应用")} onClose={onClose}>
      {!app && (
        <Field label="Key" hint={t("应用稳定标识，例如 legal-platform")}>
          <input value={key} onChange={(e) => setKey(e.target.value)} />
        </Field>
      )}
      <Field label={t("名称")}>
        <input value={name} onChange={(e) => setName(e.target.value)} />
      </Field>
      <Field label={t("允许的 Virtual Models")} hint={t("不勾选任何 = 允许全部")}>
        <div style={{ display: 'flex', gap: 10, flexWrap: 'wrap' }}>
          {vms.map((vm) => (
            <label key={vm.key} style={{ fontSize: 13 }}>
              <input type="checkbox" checked={allowed.includes(vm.key)} onChange={() => toggleVm(vm.key)} /> {vm.key}
            </label>
          ))}
        </div>
      </Field>
      <Field label={t("允许直连物理模型")}>
        <label style={{ fontSize: 13 }}>
          <input type="checkbox" checked={allowDirect} onChange={(e) => setAllowDirect(e.target.checked)} /> {t("允许（仅管理/调试场景建议开启）")}</label>
      </Field>
      <div className="field-row">
        <Field label={t("月度预算（microunits）")} hint="1 USD = 1,000,000">
          <input value={budget} onChange={(e) => setBudget(e.target.value)} placeholder={t("如 5000000 = 5 USD")} />
        </Field>
        <Field label={t("RPM（每分钟请求数）")}>
          <input value={rpm} onChange={(e) => setRpm(e.target.value)} placeholder={t("如 60")} />
        </Field>
      </div>
      <Field label={t("月度成本上限（microunits）")}>
        <input value={monthlyCost} onChange={(e) => setMonthlyCost(e.target.value)} />
      </Field>
      {error && <p style={{ color: 'var(--err)', fontSize: 12.5 }}>{error}</p>}
      <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8 }}>
        <Button onClick={onClose}>{t("取消")}</Button>
        <Button variant="primary" onClick={save} disabled={saving || (!app && !key)}>
          {saving ? t("保存中…") : t("保存")}
        </Button>
      </div>
    </Modal>
  )
}

function KeysPanel({ app, onClose, onChanged }: { app: ApplicationDto; onClose: () => void; onChanged: () => void }) {
  const [keys, setKeys] = useState<ApiKeyDto[]>([])
  const [creating, setCreating] = useState(false)
  const [plaintext, setPlaintext] = useState('')
  const [error, setError] = useState('')
  const [copied, setCopied] = useState(false)

  const load = () => api.get<ApiKeyDto[]>(`/api/v1/admin/applications/${app.id}/keys`).then(setKeys)
  useEffect(() => {
    load()
  }, [])

  const create = async () => {
    setError('')
    try {
      const r = await api.post<{ key: ApiKeyDto; plaintext: string }>(`/api/v1/admin/applications/${app.id}/keys`, {
        name: 'default',
      })
      setPlaintext(r.plaintext)
      setCreating(false)
      setCopied(false)
      load()
      onChanged()
    } catch (e: any) {
      setError(e.message)
    }
  }

  const revoke = async (k: ApiKeyDto) => {
    if (!confirm(t("撤销 Key {0}？撤销后使用该 Key 的请求立即被拒绝。", [k.maskedKey]))) return
    try {
      await api.del(`/api/v1/admin/applications/${app.id}/keys/${k.id}`)
      load()
      onChanged()
    } catch (e: any) {
      setError(e.message)
    }
  }

  return (
    <Modal title={`API Keys · ${app.name}`} onClose={onClose} wide>
      {plaintext && (
        <div className="key-reveal">
          <b>{t("请立即保存此 Key")}</b> {t("— 关闭后无法再次查看（服务端仅存 hash）。")}<code>{plaintext}</code>
          <Button
            variant="primary"
            onClick={() => {
              navigator.clipboard.writeText(plaintext)
              setCopied(true)
            }}
          >
            {copied ? t("已复制 ✓") : t("复制 Key")}
          </Button>
        </div>
      )}
      <Table head={[t("名称"), 'Key', t("最近使用"), t("创建时间"), t("状态"), t("操作")]}>
        {keys.map((k) => (
          <tr key={k.id}>
            <td>{k.name}</td>
            <td className="mono">{k.maskedKey}</td>
            <td className="dim">{formatTime(k.lastUsedAt)}</td>
            <td className="dim">{formatTime(k.createdAt)}</td>
            <td>{k.revokedAt ? <Badge tone="err">{t("已撤销")}</Badge> : <Badge tone="ok">{t("有效")}</Badge>}</td>
            <td>{!k.revokedAt && <Button variant="ghost" onClick={() => revoke(k)}>{t("撤销")}</Button>}</td>
          </tr>
        ))}
      </Table>
      {error && <p style={{ color: 'var(--err)', fontSize: 12.5 }}>{error}</p>}
      <div style={{ marginTop: 12 }}>
        <Button variant="primary" onClick={() => setCreating(true)} disabled={creating}>
          {t("+ 创建新 Key")}</Button>
        {creating && <span className="dim" style={{ marginLeft: 10 }}>{t("确认创建？")}<Button variant="ghost" onClick={create}>{t("确认")}</Button></span>}
      </div>
      <p className="dim" style={{ fontSize: 12 }}>
        {t("支持 Key 滚动：先创建新 Key → 应用切换 → 验证 → 撤销旧 Key。")}</p>
    </Modal>
  )
}
