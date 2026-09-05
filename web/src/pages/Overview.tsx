import { useEffect, useState } from 'react'
import { api } from '../api/client'
import type { UsageSummary, TimeseriesPoint, GroupUsage, ProviderDto } from '../api/types'
import { formatCost, formatTokens, formatTime } from '../api/types'
import { Card, Stat, Table, HealthBadge, Spinner, EmptyState } from '../components/ui'
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
  const [providers, setProviders] = useState<ProviderDto[]>([])
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    Promise.all([
      api.get<UsageSummary>('/api/v1/admin/usage/summary'),
      api.get<TimeseriesPoint[]>('/api/v1/admin/usage/timeseries?bucket=day'),
      api.get<GroupUsage[]>('/api/v1/admin/usage/by-model'),
      api.get<GroupUsage[]>('/api/v1/admin/usage/by-application'),
      api.get<ProviderDto[]>('/api/v1/admin/providers'),
    ])
      .then(([s, t, m, a, p]) => {
        setSummary(s)
        setSeries(t)
        setByModel(m)
        setByApp(a)
        setProviders(p)
      })
      .finally(() => setLoading(false))
  }, [])

  if (loading) return <Spinner label="加载中…" />

  return (
    <>
      <div className="page-head">
        <div>
          <h2>总览</h2>
          <div className="page-sub">统一 AI 网关的请求、Token、成本与健康状态</div>
        </div>
      </div>

      {summary.requests === 0 && providers.length === 0 && (
        <Card title="开始使用">
          <div className="empty-hint">
            <ol style={{ lineHeight: 2, margin: 0, paddingLeft: 18 }}>
              <li>添加 Provider（OpenAI 兼容接口）</li>
              <li>测试连接并发现模型</li>
              <li>为 Virtual Model 绑定物理模型</li>
              <li>创建 Application 与 API Key</li>
              <li>在 Playground 中试用</li>
            </ol>
          </div>
        </Card>
      )}

      <div className="stat-grid">
        <Stat label="总请求" value={summary.requests} sub={`成功 ${summary.successRequests} · 失败 ${summary.failedRequests}`} />
        <Stat label="成功率" value={`${(summary.successRate * 100).toFixed(1)}%`} />
        <Stat label="总 Token" value={formatTokens(summary.totalTokens)} sub={`输入 ${formatTokens(summary.inputTokens)} / 输出 ${formatTokens(summary.outputTokens)}`} />
        <Stat label="总成本" value={formatCost(summary.costMicrounits, summary.currency)} />
        <Stat label="P95 延迟" value={summary.p95LatencyMs != null ? `${summary.p95LatencyMs} ms` : '-'} sub={`P50 ${summary.p50LatencyMs ?? '-'} ms`} />
        <Stat label="平均 TTFT" value={summary.avgTtftMs != null ? `${summary.avgTtftMs} ms` : '-'} />
      </div>

      <Card title="每日请求">
        <BarChart data={series} valueKey="requests" />
      </Card>

      <div className="grid-2">
        <Card title="按模型">
          {byModel.length ? (
            <Table head={['模型', '请求', 'Tokens', '成本']}>
              {byModel.map((g) => (
                <tr key={g.group}>
                  <td className="mono">{g.group}</td>
                  <td>{g.requests}</td>
                  <td>{formatTokens(g.totalTokens)}</td>
                  <td>{formatCost(g.costMicrounits)}</td>
                </tr>
              ))}
            </Table>
          ) : (
            <EmptyState title="暂无数据" />
          )}
        </Card>
        <Card title="按应用">
          {byApp.length ? (
            <Table head={['应用', '请求', 'Tokens', '成本']}>
              {byApp.map((g) => (
                <tr key={g.group}>
                  <td className="mono">{g.group}</td>
                  <td>{g.requests}</td>
                  <td>{formatTokens(g.totalTokens)}</td>
                  <td>{formatCost(g.costMicrounits)}</td>
                </tr>
              ))}
            </Table>
          ) : (
            <EmptyState title="暂无数据" />
          )}
        </Card>
      </div>

      <Card title="Provider 健康">
        {providers.length ? (
          <Table head={['Provider', '类型', '健康', '模型数', '最近检测']}>
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
          <EmptyState title="尚未配置 Provider" hint="前往 Providers 页面添加" />
        )}
      </Card>
    </>
  )
}
