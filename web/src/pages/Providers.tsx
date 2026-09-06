import { t } from '../i18n'
import { useEffect, useState } from 'react'
import { api, qs } from '../api/client'
import type { ProviderDto, ModelDto } from '../api/types'
import { formatTime } from '../api/types'
import { Button, Card, Field, Modal, Table, HealthBadge, Toast, Spinner, EmptyState, Badge } from '../components/ui'

const KINDS = [
  ['openai_compatible', 'OpenAI Compatible'],
  ['openai', 'OpenAI'],
  ['ollama', 'Ollama'],
  ['anthropic', t("Anthropic（Adapter 待接入）")],
  ['gemini', t("Gemini（Adapter 待接入）")],
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
    if (tone !== 'err') setTimeout(() => setToast({ message: '', tone }), 2600)
  }

  const load = () => api.get<ProviderDto[]>('/api/v1/admin/providers').then(setProviders)

  useEffect(() => {
    load().catch((e) => notify(e.message, 'err')).finally(() => setLoading(false))
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
      notify(r.ok ? t("连接正常（{0}ms）", [r.latencyMs]) : t("失败：{0}", [r.error ?? 'unknown']), r.ok ? 'ok' : 'err')
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
      notify(t("发现 {0} 个模型（新增 {1} / 更新 {2}）", [r.discovered, r.created, r.updated]))
      setDiscovered(r.models)
      await load()
    } catch (e: any) {
      notify(e.message, 'err')
    }
  }

  const remove = async (p: ProviderDto) => {
    if (!confirm(t("删除 Provider「{0}」？", [p.name]))) return
    try {
      await api.del(`/api/v1/admin/providers/${p.id}`)
      notify(t("已删除"))
      await load()
    } catch (e: any) {
      notify(e.message, 'err')
    }
  }

  if (loading) return <Spinner label={t("加载中…")} />

  return (
    <>
      <div className="page-head">
        <div>
          <h2>{t('服务商')}</h2>
          <div className="page-sub">{t("统一接入模型提供方；凭据保存在系统 Secret Store（macOS Keychain）")}</div>
        </div>
        <Button variant="primary" onClick={() => setCreating(true)}>
          {t("+ 添加 Provider")}</Button>
      </div>

      <Card>
        {providers.length ? (
          <Table head={[t("名称"), t("类型"), 'Base URL', t("模型"), t("健康"), t("最近检测"), t("状态"), t("操作")]}>
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
                <td>{p.enabled ? <Badge tone="ok">{t("启用")}</Badge> : <Badge tone="muted">{t("停用")}</Badge>}</td>
                <td>
                  <div style={{ display: 'flex', gap: 4, flexWrap: 'wrap' }}>
                    <Button variant="ghost" onClick={() => test(p)}>
                      {t("测试")}</Button>
                    <Button variant="ghost" onClick={() => discover(p)}>
                      {t("发现模型")}</Button>
                    <Button variant="ghost" onClick={() => setEditing(p)}>
                      {t("编辑")}</Button>
                    <Button variant="ghost" onClick={() => toggle(p)}>
                      {p.enabled ? t("停用") : t("启用")}
                    </Button>
                    <Button variant="ghost" onClick={() => remove(p)}>
                      {t("删除")}</Button>
                  </div>
                </td>
              </tr>
            ))}
          </Table>
        ) : (
          <EmptyState title={t("还没有 Provider")} hint={t("添加一个 OpenAI 兼容 Provider 开始使用")} />
        )}
      </Card>

      {creating && (
        <ProviderForm
          title={t("添加 Provider")}
          onClose={() => setCreating(false)}
          onSaved={(p) => {
            setCreating(false)
            notify(t("Provider「{0}」已创建", [p.name]))
            load()
          }}
        />
      )}
      {editing && (
        <ProviderForm
          title={t("编辑 {0}", [editing.name])}
          provider={editing}
          onClose={() => setEditing(null)}
          onSaved={(p) => {
            setEditing(null)
            notify(t("Provider「{0}」已保存", [p.name]))
            load()
          }}
        />
      )}
      {discovered && (
        <Modal title={t("发现的模型")} onClose={() => setDiscovered(null)} wide>
          <Table head={['Model Key', t("类型"), t("状态")]}>
            {discovered.map((m) => (
              <tr key={m.id}>
                <td className="mono">{m.modelKey}</td>
                <td>{m.modelType}</td>
                <td>{m.enabled ? <Badge tone="ok">{t("启用")}</Badge> : <Badge tone="muted">{t("停用")}</Badge>}</td>
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
        <Field label={t("Key（稳定标识）")} hint={t("用于程序引用，例如 openai-primary")}>
          <input value={key} onChange={(e) => setKey(e.target.value)} placeholder="openai-primary" />
        </Field>
      )}
      <Field label={t("名称")}>
        <input value={name} onChange={(e) => setName(e.target.value)} placeholder={t("显示名称")} />
      </Field>
      <Field label={t("类型")}>
        <select value={kind} onChange={(e) => setKind(e.target.value)}>
          {KINDS.map(([v, l]) => (
            <option key={v} value={v}>
              {l}
            </option>
          ))}
        </select>
      </Field>
      <Field label="Base URL" hint={t("例如 https://api.openai.com/v1 或 http://127.0.0.1:11434/v1")}>
        <input value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)} placeholder="https://…/v1" />
      </Field>
      <Field label={provider ? t("API Key（留空保持不变，填写即轮换）") : 'API Key'}>
        <input type="password" value={apiKey} onChange={(e) => setApiKey(e.target.value)} placeholder={provider?.credentialConfigured ? t("已配置") : 'sk-…'} />
      </Field>
      <Field label={t("超时（毫秒）")}>
        <input value={timeoutMs} onChange={(e) => setTimeoutMs(e.target.value)} />
      </Field>
      {error && <p style={{ color: 'var(--err)', fontSize: 12.5 }}>{error}</p>}
      <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8 }}>
        <Button onClick={onClose}>{t("取消")}</Button>
        <Button variant="primary" onClick={save} disabled={saving || !name || !baseUrl}>
          {saving ? t("保存中…") : t("保存")}
        </Button>
      </div>
    </Modal>
  )
}
