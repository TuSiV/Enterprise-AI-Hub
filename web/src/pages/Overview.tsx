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
import { useEffect, useState } from 'react'
import { api } from '../api/client'
import type { UsageSummary, TimeseriesPoint, GroupUsage, ProviderDto } from '../api/types'
import { formatCost, formatTokens, formatTime } from '../api/types'
import { Card, Stat, Table, HealthBadge, Spinner, EmptyState, Button } from '../components/ui'
import { BarChart } from '../components/charts'

export default function Overview() {
  const [summary, setSummary] = useState<UsageSummary>({
    requests: 0, successRequests: 0, failedRequests: 0, inputTokens: 0, outputTokens: 0,
    totalTokens: 0, costMicrounits: 0, currency: 'USD', successRate: 0,
    p50LatencyMs: null, p95LatencyMs: null, avgTtftMs: null, cacheHitRate: 0,
  })
  const [series, setSeries] = useState<TimeseriesPoint[]>([])
  const [byModel, setByModel] = useState<GroupUsage[]>([])
  const [byApp, setByApp] = useState<GroupUsage[]>([])
  const [byUser, setByUser] = useState<GroupUsage[]>([])
  const [providers, setProviders] = useState<ProviderDto[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  const [metric, setMetric] = useState('requests')

  const load = () => {
    setLoading(true)
    setError('')
    Promise.all([
      api.get<UsageSummary>('/api/v1/admin/usage/summary'),
      api.get<TimeseriesPoint[]>('/api/v1/admin/usage/timeseries?bucket=day'),
      api.get<GroupUsage[]>('/api/v1/admin/usage/by-model'),
      api.get<GroupUsage[]>('/api/v1/admin/usage/by-application'),
      api.get<GroupUsage[]>('/api/v1/admin/usage/by-user'),
      api.get<ProviderDto[]>('/api/v1/admin/providers'),
    ])
      .then(([s, t, m, a, u, p]) => {
        setSummary(s)
        setSeries(t)
        setByModel(m)
        setByApp(a)
        setByUser(u)
        setProviders(p)
      })
      .catch(e => setError(e.message))
      .finally(() => setLoading(false))
  }
  useEffect(load, [])

  if (loading) return <Spinner label={t("加载中…")} />

  return (
    <>
      <div className="page-head">
        <div>
          <h2>{t("总览")}</h2>
          <div className="page-sub">{t("统一 AI 网关的请求、Token、成本与健康状态")}</div>
        </div>
      </div>

      {error && <div className="error-banner" role="alert">{error}<Button onClick={load}>{t('重试')}</Button></div>}
      <div className="overview-intro"><div><span className="eyebrow">AI GATEWAY / {t('运行总览')}</span><h3>{t('每一次调用，都清晰可见。')}</h3><p>{t('从模型使用到服务健康，在一个工作空间掌握全局。')}</p></div><a className="btn btn-primary" href="#/playground">{t('在 Playground 中试用')} →</a></div>
      {summary.requests === 0 && providers.length === 0 && (
        <Card title={t("开始使用")}>
          <div className="empty-hint">
            <ol style={{ lineHeight: 2, margin: 0, paddingLeft: 18 }}>
              <li>{t("添加 Provider（OpenAI 兼容接口）")}</li>
              <li>{t("测试连接并发现模型")}</li>
              <li>{t("为 Virtual Model 绑定物理模型")}</li>
              <li>{t("创建 Application 与 API Key")}</li>
              <li>{t("在 Playground 中试用")}</li>
            </ol>
          </div>
        </Card>
      )}

      <div className="stat-grid">
        <Stat label={t("总请求")} value={number(summary.requests)} sub={t("成功 {0} · 失败 {1}", [summary.successRequests, summary.failedRequests])} />
        <Stat label={t("成功率")} value={`${(summary.successRate * 100).toFixed(1)}%`} />
        <Stat label={t("总 Token")} value={formatTokens(summary.totalTokens)} sub={t("输入 {0} / 输出 {1}", [formatTokens(summary.inputTokens), formatTokens(summary.outputTokens)])} />
        <Stat label={t("总成本")} value={formatCost(summary.costMicrounits, summary.currency)} />
        <Stat label={t("P95 延迟")} value={summary.p95LatencyMs != null ? `${summary.p95LatencyMs} ms` : '-'} sub={`P50 ${summary.p50LatencyMs ?? '-'} ms`} />
        <Stat label={t("平均 TTFT")} value={summary.avgTtftMs != null ? `${summary.avgTtftMs} ms` : '-'} />
      </div>

      <Card title={t('用量趋势')} actions={<div className="view-switch">{['requests', 'tokens', 'costMicrounits'].map(key => <button key={key} className={metric === key ? 'active' : ''} aria-pressed={metric === key} onClick={() => setMetric(key)}>{t(key === 'requests' ? '请求' : key === 'tokens' ? '总 Token' : '成本')}</button>)}</div>}>
        <BarChart data={series.map(point => ({ ...point, tokens: point.inputTokens + point.outputTokens }))} valueKey={metric} label={value => metric === 'costMicrounits' ? formatCost(value, summary.currency) : formatTokens(value)} />
      </Card>

      <div className="grid-2">
        <Card title={t("按模型")}>
          {byModel.length ? (
            <Table head={[t("模型"), t("请求"), 'Tokens', t("成本")]}>
              {byModel.map((g) => (
                <tr key={g.group}>
                  <td className="mono">{g.group}</td>
                  <td>{g.requests}</td>
                  <td>{formatTokens(g.totalTokens)}</td>
                  <td>{formatCost(g.costMicrounits, summary.currency)}</td>
                </tr>
              ))}
            </Table>
          ) : (
            <EmptyState title={t("暂无数据")} />
          )}
        </Card>
        <Card title={t("按应用")}>
          {byApp.length ? (
            <Table head={[t("应用"), t("请求"), 'Tokens', t("成本")]}>
              {byApp.map((g) => (
                <tr key={g.group}>
                  <td className="mono">{g.group}</td>
                  <td>{g.requests}</td>
                  <td>{formatTokens(g.totalTokens)}</td>
                  <td>{formatCost(g.costMicrounits, summary.currency)}</td>
                </tr>
              ))}
            </Table>
          ) : (
            <EmptyState title={t("暂无数据")} />
          )}
        </Card>
      </div>

      <div className="grid-2">
        <Card title={t("按用户")}>
          {byUser.length ? (
            <Table head={[t("用户"), t("请求"), 'Tokens', t("成本")]}>
              {byUser.map((g) => (
                <tr key={g.group}>
                  <td className="mono">{g.group}</td>
                  <td>{g.requests}</td>
                  <td>{formatTokens(g.totalTokens)}</td>
                  <td>{formatCost(g.costMicrounits, summary.currency)}</td>
                </tr>
              ))}
            </Table>
          ) : (
            <EmptyState title={t("暂无数据")} hint={t("调用时传 OpenAI user 字段或 X-AiHub-User 头即可按用户归因")} />
          )}
        </Card>
      </div>

      <div className="health-summary"><span>{t('缓存命中率')} <strong>{(summary.cacheHitRate * 100).toFixed(1)}%</strong></span><span>{t('健康服务商')} <strong>{providers.filter(p => p.health === 'healthy').length} / {providers.length}</strong></span></div>
      <Card title={t("Provider 健康")}>
        {providers.length ? (
          <Table head={['Provider', t("类型"), t("健康"), t("模型数"), t("最近检测")]}>
            {providers.map((p) => (
              <tr key={p.id}>
                <td>
                  {p.name} <span className="dim mono">({p.key})</span>
                </td>
                <td className="mono">{p.kind}</td>
                <td>
                  <HealthBadge health={p.health} />
                </td>
                <td>{p.modelCount}</td>
                <td className="dim">{formatTime(p.lastHealthCheckAt)}</td>
              </tr>
            ))}
          </Table>
        ) : (
          <EmptyState title={t("尚未配置 Provider")} hint={t("前往 Providers 页面添加")} />
        )}
      </Card>
    </>
  )
}
