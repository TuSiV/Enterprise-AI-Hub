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

import { t, number } from '../i18n'
import { useEffect, useState, useMemo } from 'react'
import { api } from '../api/client'
import type { ModelDto, Pricing, PricingPreset } from '../api/types'
import { Badge, Button, Card, Field, Modal, Table, Toast, Spinner, EmptyState } from '../components/ui'

export default function Models() {
  const [models, setModels] = useState<ModelDto[]>([])
  const [providers, setProviders] = useState<{ id: string; name: string }[]>([])
  const [providerFilter, setProviderFilter] = useState('')
  const [typeFilter, setTypeFilter] = useState('')
  const [search, setSearch] = useState('')
  const [statusFilter, setStatusFilter] = useState('')
  const [sort, setSort] = useState('name')
  const [view, setView] = useState<'cards' | 'table'>('cards')
  const [error, setError] = useState('')
  const [detail, setDetail] = useState<ModelDto | null>(null)
  const filtered = useMemo(() => models.filter(m =>
    (!providerFilter || m.providerId === providerFilter) && (!typeFilter || m.modelType === typeFilter) &&
    (!statusFilter || String(m.enabled) === statusFilter) &&
    `${m.displayName} ${m.modelKey} ${m.providerName ?? ''}`.toLowerCase().includes(search.toLowerCase())
  ).sort((a, b) => sort === 'context' ? (b.contextWindow ?? 0) - (a.contextWindow ?? 0) : sort === 'price' ? (a.pricing?.currency ?? 'USD').localeCompare(b.pricing?.currency ?? 'USD') || (unitPrice(a) ?? Infinity) - (unitPrice(b) ?? Infinity) : a.displayName.localeCompare(b.displayName)), [models, providerFilter, typeFilter, statusFilter, search, sort])
  const [loading, setLoading] = useState(true)
  const [editing, setEditing] = useState<ModelDto | null>(null)
  const [creating, setCreating] = useState(false)
  const [toast, setToast] = useState<{ message: string; tone: 'ok' | 'err' }>({ message: '', tone: 'ok' })

  const notify = (message: string, tone: 'ok' | 'err' = 'ok') => {
    setToast({ message, tone })
    if (tone !== 'err') setTimeout(() => setToast({ message: '', tone }), 2600)
  }

  const load = async () => {
    const data = await api.get<ModelDto[]>('/api/v1/admin/models')
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
      .catch(e => setError(e.message))
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
    if (!confirm(t("删除模型「{0}」？", [m.displayName]))) return
    try {
      await api.del(`/api/v1/admin/models/${m.id}`)
      notify(t("已删除"))
      reload()
    } catch (e: any) {
      notify(e.message, 'err')
    }
  }

  if (loading) return <Spinner label={t("加载中…")} />

  return (
    <>
      <div className="page-head">
        <div>
          <h2>{t("模型注册表")}</h2>
          <div className="page-sub">{t("物理模型与定价；Virtual Model 在此之上做路由抽象")}</div>
        </div>
        <Button variant="primary" onClick={() => setCreating(true)}>
          {t("+ 手工登记模型")}</Button>
      </div>

      <div className="catalog-summary"><span><strong>{number(models.length)}</strong> {t('模型总数')}</span><span><strong>{number(models.filter(m => m.enabled).length)}</strong> {t('启用')}</span><span><strong>{number(new Set(models.map(m => m.providerId)).size)}</strong> {t('服务商')}</span></div>
      {error && <div className="error-banner" role="alert">{error}<Button onClick={() => { setError(''); load().catch(e => setError(e.message)) }}>{t('重试')}</Button></div>}
      <div className="catalog-toolbar">
        <label className="search-field"><span className="dim">⌕</span><input aria-label={t('搜索模型')} placeholder={t('搜索模型名称、标识或服务商…')} value={search} onChange={e => setSearch(e.target.value)} /></label>
        <div className="view-switch" aria-label={t('展示方式')}><button className={view === 'cards' ? 'active' : ''} aria-pressed={view === 'cards'} onClick={() => setView('cards')}>{t('卡片')}</button><button className={view === 'table' ? 'active' : ''} aria-pressed={view === 'table'} onClick={() => setView('table')}>{t('表格')}</button></div>
      </div>
      <div className="catalog-layout">
        <aside className="catalog-filters">
          <h3>{t('筛选模型')}</h3>
          <Field label={t('服务商')}><select value={providerFilter} onChange={e => setProviderFilter(e.target.value)}><option value="">{t('全部 Provider')}</option>{providers.map(p => <option key={p.id} value={p.id}>{p.name}</option>)}</select></Field>
          <Field label={t('类型')}><select value={typeFilter} onChange={e => setTypeFilter(e.target.value)}><option value="">{t('全部类型')}</option>{Array.from(new Set(['chat', 'reasoning', 'embedding', 'rerank', 'multimodal', ...models.map(m => m.modelType)])).map(type => <option key={type} value={type}>{type}</option>)}</select></Field>
          <Field label={t('状态')}><select value={statusFilter} onChange={e => setStatusFilter(e.target.value)}><option value="">{t('全部状态')}</option><option value="true">{t('启用')}</option><option value="false">{t('停用')}</option></select></Field>
          <Button variant="ghost" onClick={() => { setProviderFilter(''); setTypeFilter(''); setStatusFilter(''); setSearch('') }}>{t('清除筛选')}</Button>
          <p className="catalog-note">{t('价格按每百万 Token 展示；未配置的数据标记为未提供。')}</p>
        </aside>
        <section className="catalog-results">
          <div className="results-head"><span className="dim" role="status">{t('找到 {0} 个模型', [number(filtered.length)])}</span><select aria-label={t('排序')} value={sort} onChange={e => setSort(e.target.value)}><option value="name">{t('名称排序')}</option><option value="context">{t('上下文从大到小')}</option><option value="price">{t('输入价格从低到高（同币种比较）')}</option></select></div>
          {filtered.length ? view === 'cards' ? <div className="model-grid">{filtered.map(m => <article className="model-card" key={m.id}>
            <div className="model-card-top"><span className="provider-avatar">{(m.providerName ?? m.providerKey ?? 'AI').slice(0,2).toUpperCase()}</span><div><span className="provider-name">{m.providerName ?? m.providerKey ?? t('未知')}</span><div className="model-type">{m.modelType}</div></div><Badge tone={m.enabled ? 'ok' : 'muted'}>{t(m.enabled ? '启用' : '停用')}</Badge></div>
            <h3><button className="model-title" onClick={() => setDetail(m)}>{m.displayName}</button></h3><div className="model-key mono">{m.modelKey}</div>
            <CapabilityTags value={m.capabilities} /><div className="model-specs"><div><span>{t('上下文窗口')}</span><strong>{m.contextWindow == null ? '—' : number(m.contextWindow)}</strong></div><div><span>{t('最大输出')}</span><strong>{m.maxOutputTokens == null ? '—' : number(m.maxOutputTokens)}</strong></div></div>
            <div className="model-pricing"><div><span>{t('输入')}</span><strong>{price(m, 'input')}</strong></div><div><span>{t('输出')}</span><strong>{price(m, 'output')}</strong></div><div><span>{t('缓存输入')}</span><strong>{price(m, 'cachedInput')}</strong></div></div>
            <div className="model-card-foot"><Badge tone="muted">{t(m.discovered ? '发现' : '手工')}</Badge><Button variant="ghost" onClick={() => setDetail(m)}>{t('查看详情')} →</Button><Button variant="ghost" onClick={() => setEditing(m)}>{t('编辑')}</Button></div>
          </article>)}</div> : <Card><Table head={[t('模型'), t('服务商'), t('类型'), t('上下文'), t('最大输出'), t('输入'), t('输出'), t('缓存输入'), t('状态'), t('操作')]}>{filtered.map(m => <tr key={m.id}><td><button className="model-title" onClick={() => setDetail(m)}>{m.displayName}</button><div className="dim mono">{m.modelKey}</div></td><td>{m.providerName}</td><td>{m.modelType}</td><td>{m.contextWindow == null ? '—' : number(m.contextWindow)}</td><td>{m.maxOutputTokens == null ? '—' : number(m.maxOutputTokens)}</td><td>{price(m,'input')}</td><td>{price(m,'output')}</td><td>{price(m,'cachedInput')}</td><td><Badge tone={m.enabled ? 'ok' : 'muted'}>{t(m.enabled ? '启用' : '停用')}</Badge></td><td><Button variant="ghost" onClick={() => setEditing(m)}>{t('编辑')}</Button><Button variant="ghost" onClick={() => toggle(m)}>{t(m.enabled ? '停用模型' : '启用模型')}</Button><Button variant="danger" onClick={() => remove(m)}>{t('删除')}</Button></td></tr>)}</Table></Card> : <Card><EmptyState title={t('没有模型')} hint={models.length ? t('尝试其他搜索词或清除筛选条件。') : t('在 Provider 页面使用「发现模型」自动拉取，或手工登记')} /></Card>}
        </section>
      </div>
      {detail && <Modal title={detail.displayName} onClose={() => setDetail(null)} wide><div className="dim mono">{detail.modelKey}</div><Table head={[t('字段'),t('值')]}>{[[t('服务商'),detail.providerName],[t('类型'),detail.modelType],[t('上下文'),detail.contextWindow == null ? '—' : number(detail.contextWindow)],[t('最大输出'),detail.maxOutputTokens == null ? '—' : number(detail.maxOutputTokens)],[t('输入'),price(detail,'input')],[t('输出'),price(detail,'output')],[t('缓存输入'),price(detail,'cachedInput')],[t('推理价格'),price(detail,'reasoning')],[t('创建时间'),new Date(detail.createdAt).toLocaleString(document.documentElement.lang)],[t('更新时间'),new Date(detail.updatedAt).toLocaleString(document.documentElement.lang)]].map(([label,value]) => <tr key={label}><td>{label}</td><td>{value ?? t('未提供')}</td></tr>)}</Table><h4>{t('能力')}</h4><CapabilityTags value={detail.capabilities} /><pre className="json">{JSON.stringify(detail.capabilities ?? {}, null, 2)}</pre><h4>{t('元数据')}</h4><pre className="json">{JSON.stringify(detail.metadata ?? {}, null, 2)}</pre><div className="detail-actions"><Button onClick={() => { setDetail(null); setEditing(detail) }}>{t('编辑')}</Button><Button onClick={() => { toggle(detail); setDetail(null) }}>{t(detail.enabled ? '停用模型' : '启用模型')}</Button><Button variant="danger" onClick={() => { remove(detail); setDetail(null) }}>{t('删除')}</Button></div></Modal>}

      {creating && (
        <ModelForm
          providers={providers}
          onClose={() => setCreating(false)}
          onSaved={() => {
            setCreating(false)
            notify(t("模型已登记"))
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
            notify(t("模型已保存"))
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
  const [maxOutput, setMaxOutput] = useState(String(model?.maxOutputTokens ?? ''))
  const [pricing, setPricing] = useState<Pricing>(() => {
    const original = model?.pricing ?? { currency: 'USD', unitTokens: 1000000 }
    const factor = 1000000 / (original.unitTokens && original.unitTokens > 0 ? original.unitTokens : 1000000)
    return { ...original, unitTokens: 1000000, input: original.input == null ? original.input : original.input * factor, output: original.output == null ? original.output : original.output * factor, cachedInput: original.cachedInput == null ? original.cachedInput : original.cachedInput * factor, reasoning: original.reasoning == null ? original.reasoning : original.reasoning * factor }
  })
  const [error, setError] = useState('')
  const [saving, setSaving] = useState(false)
  const [presets, setPresets] = useState<PricingPreset[]>([])
  const [selectedPreset, setSelectedPreset] = useState('')

  useEffect(() => {
    // api.get 已解包 {data, meta} 信封，直接返回数组
    api.get<PricingPreset[]>('/api/v1/admin/pricing-presets')
      .then(res => setPresets(Array.isArray(res) ? res : []))
      .catch(() => {})
  }, [])

  const applyPreset = (presetKey: string) => {
    setSelectedPreset(presetKey)
    const preset = presets.find(p => p.modelKey === presetKey)
    if (preset) {
      setModelKey(preset.modelKey)
      setDisplayName(preset.displayName)
      setContextWindow(preset.contextWindow != null ? String(preset.contextWindow) : '')
      setMaxOutput(preset.maxOutput != null && preset.maxOutput > 0 ? String(preset.maxOutput) : '')
      setPricing({
        currency: preset.pricing.currency ?? 'USD',
        unitTokens: 1000000,
        input: preset.pricing.input ?? null,
        output: preset.pricing.output ?? null,
        cachedInput: preset.pricing.cachedInput ?? null,
        reasoning: preset.pricing.reasoning ?? null,
      })
      const typeHint = preset.modelKey.includes('embedding')
        ? 'embedding'
        : preset.pricing.reasoning != null ? 'reasoning' : 'chat'
      setModelType(typeHint)
    }
  }

  const save = async () => {
    setSaving(true)
    setError('')
    const ctx = Number(contextWindow)
    const mx = Number(maxOutput)
    const payload: any = {
      displayName,
      modelType,
      contextWindow: ctx > 0 ? ctx : null,
      maxOutputTokens: mx > 0 ? mx : null,
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
    <Modal title={model ? t("编辑 {0}", [model.displayName]) : t("登记模型")} onClose={onClose}>
      {!model && (
        <>
          {presets.length > 0 && (
            <Field label={t("快速填充预置价格")} hint={t("选择后自动填充模型信息和价格，可手动修改")}>
              <select value={selectedPreset} onChange={(e) => applyPreset(e.target.value)}>
                <option value="">{t("不使用预置（手动填写）")}</option>
                {Object.entries(
                  presets.reduce((acc, p) => {
                    if (!acc[p.provider]) acc[p.provider] = []
                    acc[p.provider].push(p)
                    return acc
                  }, {} as Record<string, PricingPreset[]>)
                ).map(([provider, items]) => (
                  <optgroup key={provider} label={provider}>
                    {items.map(p => (
                      <option key={p.modelKey} value={p.modelKey}>
                        {p.displayName}{p.pricing.input != null ? ` ($${p.pricing.input}/${p.pricing.output ?? '?'})` : ''}
                      </option>
                    ))}
                  </optgroup>
                ))}
              </select>
            </Field>
          )}
          <Field label="Provider">
            <select value={providerId} onChange={(e) => setProviderId(e.target.value)}>
              {providers.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </select>
          </Field>
          <Field label="Model Key" hint={t("Provider 侧真实模型名，例如 gpt-4o-mini")}>
            <input value={modelKey} onChange={(e) => setModelKey(e.target.value)} />
          </Field>
        </>
      )}
      <Field label={t("显示名称")}>
        <input value={displayName} onChange={(e) => setDisplayName(e.target.value)} />
      </Field>
      <div className="field-row">
        <Field label={t("类型")}>
          <select value={modelType} onChange={(e) => setModelType(e.target.value)}>
            {['chat', 'reasoning', 'embedding', 'rerank', 'multimodal'].map((t) => (
              <option key={t}>{t}</option>
            ))}
          </select>
        </Field>
        <Field label={t("上下文窗口")}>
          <input value={contextWindow} onChange={(e) => setContextWindow(e.target.value)} placeholder={t("如 128000")} />
        </Field>
        <Field label={t("最大输出")}>
          <input value={maxOutput} onChange={(e) => setMaxOutput(e.target.value)} placeholder={t("如 16384")} />
        </Field>
      </div>
      <div className="field-row">
        <Field label={t("输入价格 / 1M Tokens")}>
          <input value={pricing.input ?? ''} onChange={(e) => setPricing({ ...pricing, input: Number(e.target.value) || null })} />
        </Field>
        <Field label={t("输出价格 / 1M Tokens")}>
          <input value={pricing.output ?? ''} onChange={(e) => setPricing({ ...pricing, output: Number(e.target.value) || null })} />
        </Field>
      </div>
      {error && <p style={{ color: 'var(--err)', fontSize: 12.5 }}>{error}</p>}
      <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8 }}>
        <Button onClick={onClose}>{t("取消")}</Button>
        <Button variant="primary" onClick={save} disabled={saving || (!model && (!modelKey || !displayName))}>
          {saving ? t("保存中…") : t("保存")}
        </Button>
      </div>
    </Modal>
  )
}

function unitPrice(model: ModelDto): number | null {
  if (model.pricing?.input == null || (model.pricing.unitTokens ?? 1_000_000) <= 0) return null
  return model.pricing.input * 1_000_000 / (model.pricing.unitTokens ?? 1_000_000)
}
function price(model: ModelDto, key: 'input' | 'output' | 'cachedInput' | 'reasoning') {
  const pricing = model.pricing
  if (pricing?.[key] == null || (pricing.unitTokens ?? 1_000_000) <= 0) return t('未提供')
  const value = pricing[key]! * 1_000_000 / (pricing.unitTokens ?? 1_000_000)
  return `${pricing.currency ?? 'USD'} ${value.toLocaleString(document.documentElement.lang, { maximumFractionDigits: 6 })}`
}

function CapabilityTags({ value }: { value: unknown }) {
  const labels: Record<string, string> = { streaming: '流式输出', tools: '工具调用', toolCalling: '工具调用', vision: '视觉理解', reasoning: '推理', json: '结构化输出', structuredOutput: '结构化输出' }
  const capabilities = Array.isArray(value) ? value.filter(item => typeof item === 'string') : value && typeof value === 'object' ? Object.entries(value).filter(([,enabled]) => enabled === true).map(([key]) => key) : []
  return capabilities.length ? <div className="capability-tags">{capabilities.map((key, index) => <Badge key={index} tone="info">{t(labels[key] ?? key)}</Badge>)}</div> : null
}
