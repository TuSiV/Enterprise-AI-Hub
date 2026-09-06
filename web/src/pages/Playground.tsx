import { useEffect, useRef, useState } from 'react'
import { api } from '../api/client'
import type { VirtualModelDto, ModelDto, UsageSummary } from '../api/types'
import { formatCost } from '../api/types'
import { Button, Card, Field, Badge, Table } from '../components/ui'

interface ChatMsg {
  role: 'user' | 'assistant'
  content: string
}

// Compare Mode（§18.4）：同一输入跑多个候选并排对比
interface CompareResult {
  model: string
  content: string
  latencyMs?: number
  ttftMs?: number
  tokens?: number
  costMicrounits?: number
  error?: string
}

interface Metrics {
  resolvedModel?: string
  provider?: string
  latencyMs?: number
  ttftMs?: number
  retryCount?: number
  usage?: { input_tokens?: number; output_tokens?: number; total_tokens?: number; source?: string }
  costMicrounits?: number
}

export default function Playground() {
  const [vms, setVms] = useState<VirtualModelDto[]>([])
  const [models, setModels] = useState<ModelDto[]>([])
  const [model, setModel] = useState('')
  const [system, setSystem] = useState('')
  const [temperature, setTemperature] = useState('0.7')
  const [maxTokens, setMaxTokens] = useState('')
  const [stream, setStream] = useState(true)
  const [messages, setMessages] = useState<ChatMsg[]>([{ role: 'user', content: '你好，请介绍一下你自己。' }])
  const [input, setInput] = useState('')
  const [running, setRunning] = useState(false)
  const [reply, setReply] = useState('')
  const [reasoning, setReasoning] = useState('')
  const [metrics, setMetrics] = useState<Metrics | null>(null)
  const [error, setError] = useState('')
  const outputRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    Promise.all([
      api.get<VirtualModelDto[]>('/api/v1/admin/virtual-models'),
      api.get<ModelDto[]>('/api/v1/admin/models?enabled=true'),
    ]).then(([v, m]) => {
      setVms(v)
      setModels(m)
      const first = v.find((x) => x.enabled && x.targets.length > 0) ?? v[0]
      if (first) setModel(first.key)
    })
  }, [])

  useEffect(() => {
    outputRef.current?.scrollTo(0, outputRef.current.scrollHeight)
  }, [reply])

  const run = async () => {
    if (running || !input.trim()) return
    const allMessages: ChatMsg[] = [...messages, { role: 'user', content: input }]
    setMessages(allMessages)
    setInput('')
    setReply('')
    setReasoning('')
    setMetrics(null)
    setError('')
    setRunning(true)

    const body = {
      model,
      messages: allMessages.map((m) => ({ role: m.role, content: m.content })),
      system: system || undefined,
      temperature: Number(temperature) || undefined,
      maxTokens: maxTokens ? Number(maxTokens) : undefined,
    }

    try {
      if (!stream) {
        const r = await api.post<any>('/api/v1/admin/playground/run', body)
        setReply(r.content ?? '')
        setReasoning(r.reasoningContent ?? '')
        setMetrics({
          resolvedModel: r.resolvedModel,
          provider: r.provider,
          latencyMs: r.latencyMs,
          retryCount: r.retryCount,
          usage: r.usage,
          costMicrounits: r.costMicrounits,
        })
      } else {
        const token = localStorage.getItem('aihub_admin_token') ?? ''
        const res = await fetch('/api/v1/admin/playground/stream', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
          body: JSON.stringify(body),
        })
        if (!res.ok || !res.body) {
          const errBody = await res.json().catch(() => ({}))
          throw new Error(errBody?.error?.message ?? `请求失败 (${res.status})`)
        }
        const reader = res.body.getReader()
        const decoder = new TextDecoder()
        let buffer = ''
        let acc = ''
        let reasoningAcc = ''
        let metricsAcc: Metrics = {}
        while (true) {
          const { done, value } = await reader.read()
          if (done) break
          buffer += decoder.decode(value, { stream: true })
          const events = buffer.split('\n\n')
          buffer = events.pop() ?? ''
          for (const evt of events) {
            const line = evt.split('\n').find((l) => l.startsWith('data:'))
            if (!line) continue
            const payload = line.slice(5).trim()
            if (payload === '[DONE]') continue
            let parsed: any
            try {
              parsed = JSON.parse(payload)
            } catch {
              continue
            }
            if (parsed.type === 'content') {
              acc += parsed.delta
              setReply(acc)
            } else if (parsed.type === 'reasoning') {
              reasoningAcc += parsed.delta
              setReasoning(reasoningAcc)
            } else if (parsed.type === 'started') {
              metricsAcc = { ...metricsAcc, resolvedModel: parsed.resolvedModel, provider: parsed.provider }
              setMetrics({ ...metricsAcc })
            } else if (parsed.type === 'completed') {
              metricsAcc = {
                ...metricsAcc,
                latencyMs: parsed.latencyMs,
                ttftMs: parsed.ttftMs,
                retryCount: parsed.retryCount,
                usage: parsed.usage,
                costMicrounits: parsed.costMicrounits,
              }
              setMetrics({ ...metricsAcc })
            } else if (parsed.type === 'error') {
              throw new Error(`${parsed.code}: ${parsed.message}`)
            }
          }
        }
      }
      setMessages((prev) => [...prev, { role: 'assistant', content: accSnapshot.current || reply }])
    } catch (e: any) {
      setError(e.message ?? String(e))
    } finally {
      setRunning(false)
    }
  }

  // 保存流式累计值供收尾使用
  const accSnapshot = useRef('')
  useEffect(() => {
    accSnapshot.current = reply
  }, [reply])

  const [candidates, setCandidates] = useState('')
  const [compareResults, setCompareResults] = useState<CompareResult[] | null>(null)
  const [comparing, setComparing] = useState(false)

  const runCompare = async () => {
    if (comparing || !input.trim()) return
    const models = candidates.split(',').map((m) => m.trim()).filter(Boolean)
    if (models.length < 2) {
      setError('请输入至少两个候选模型（逗号分隔）')
      return
    }
    setComparing(true)
    setError('')
    const results: CompareResult[] = []
    for (const m of models) {
      try {
        const r = await api.post<any>('/api/v1/admin/playground/run', {
          model: m,
          messages: [...messages, { role: 'user', content: input }].map((x) => ({ role: x.role, content: x.content })),
          system: system || undefined,
          temperature: Number(temperature) || undefined,
        })
        results.push({
          model: m,
          content: r.content ?? '',
          latencyMs: r.latencyMs,
          tokens: r.usage?.total_tokens,
          costMicrounits: r.costMicrounits,
        })
      } catch (e: any) {
        results.push({ model: m, content: '', error: e.message })
      }
    }
    setCompareResults(results)
    setComparing(false)
  }

  const reset = () => {
    setMessages([])
    setReply('')
    setReasoning('')
    setMetrics(null)
    setError('')
  }

  const modelOptions = [
    ...vms.filter((v) => v.enabled).map((v) => ({ value: v.key, label: `${v.key}（virtual）` })),
    ...models.map((m) => ({ value: m.modelKey, label: `${m.modelKey}（${m.providerName ?? ''}）` })),
  ]
  const uniqueOptions = Array.from(new Map(modelOptions.map((o) => [o.value, o])).values())

  return (
    <>
      <div className="page-head">
        <div>
          <h2>Playground</h2>
          <div className="page-sub">与 Virtual Model / 物理模型直接对话；请求走同一条 Gateway 流水线（含计量与审计）</div>
        </div>
      </div>

      <div className="pg-layout">
        <div>
          <Card title="请求配置">
            <Field label="模型">
              <select value={model} onChange={(e) => setModel(e.target.value)}>
                {uniqueOptions.map((o) => (
                  <option key={o.value} value={o.value}>
                    {o.label}
                  </option>
                ))}
              </select>
            </Field>
            <Field label="System Prompt">
              <textarea value={system} onChange={(e) => setSystem(e.target.value)} rows={3} placeholder="你是一个专业的…" />
            </Field>
            <div className="field-row">
              <Field label="Temperature">
                <input value={temperature} onChange={(e) => setTemperature(e.target.value)} />
              </Field>
              <Field label="Max Tokens">
                <input value={maxTokens} onChange={(e) => setMaxTokens(e.target.value)} placeholder="默认不限制" />
              </Field>
            </div>
            <Field label="流式输出">
              <label style={{ fontSize: 13 }}>
                <input type="checkbox" checked={stream} onChange={(e) => setStream(e.target.checked)} /> SSE 流式
              </label>
            </Field>
            <Button variant="ghost" onClick={reset}>
              清空会话
            </Button>
          </Card>
        </div>

        <div>
          <Card title="对话">
            <div className="pg-messages">
              {messages.map((m, i) => (
                <div key={i} className="pg-msg">
                  <div className="pg-role">
                    <span>{m.role}</span>
                  </div>
                  <div style={{ whiteSpace: 'pre-wrap' }}>{m.content}</div>
                </div>
              ))}
            </div>
            <div className="pg-msg">
              <div className="pg-role">
                <span>user（输入）</span>
              </div>
              <textarea
                value={input}
                onChange={(e) => setInput(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) run()
                }}
                placeholder="输入消息，⌘/Ctrl+Enter 发送"
              />
            </div>
            <div style={{ marginTop: 10, display: 'flex', gap: 8, flexWrap: 'wrap' }}>
              <Button variant="primary" onClick={run} disabled={running || !input.trim()}>
                {running ? '生成中…' : '发送'}
              </Button>
              <Button onClick={runCompare} disabled={comparing || !input.trim()}>
                {comparing ? '对比中…' : 'Compare 多候选'}
              </Button>
            </div>
            <Field label="Compare 候选（逗号分隔模型 key）">
              <input value={candidates} onChange={(e) => setCandidates(e.target.value)} placeholder="general-fast, general-smart, mock-mini" />
            </Field>
          </Card>
          <Card title="回复">
            {error && <p style={{ color: 'var(--err)' }}>{error}</p>}
            <div className="pg-output" ref={outputRef}>
              {reasoning && <div className="reasoning">{reasoning}</div>}
              {reply || <span className="dim">{running ? '…' : '回复将显示在这里'}</span>}
            </div>
          </Card>
        </div>

        <div>
          <Card title="指标">
            {/* 指标卡保持不变 */}
            <div className="pg-metrics">
              <div className="row">
                <span className="dim">模型</span>
                <span className="mono">{metrics?.resolvedModel ?? '-'}</span>
              </div>
              <div className="row">
                <span className="dim">Provider</span>
                <span className="mono">{metrics?.provider ?? '-'}</span>
              </div>
              <div className="row">
                <span className="dim">延迟</span>
                <span>{metrics?.latencyMs != null ? `${metrics.latencyMs} ms` : '-'}</span>
              </div>
              <div className="row">
                <span className="dim">TTFT</span>
                <span>{metrics?.ttftMs != null ? `${metrics.ttftMs} ms` : '-'}</span>
              </div>
              <div className="row">
                <span className="dim">重试</span>
                <span>{metrics?.retryCount ?? 0}</span>
              </div>
              <div className="row">
                <span className="dim">输入 / 输出 Tokens</span>
                <span>
                  {metrics?.usage?.input_tokens ?? '-'} / {metrics?.usage?.output_tokens ?? '-'}
                </span>
              </div>
              <div className="row">
                <span className="dim">Usage 来源</span>
                <span>{metrics?.usage?.source === 'estimated' ? <Badge tone="warn">estimated</Badge> : <Badge tone="ok">provider</Badge>}</span>
              </div>
              <div className="row">
                <span className="dim">成本</span>
                <span>{formatCost(metrics?.costMicrounits)}</span>
              </div>
            </div>
          </Card>
        </div>
      </div>

      {compareResults && (
        <Card title="Compare 对比结果（同一输入）">
          <Table head={['候选', '结果', '延迟', 'Tokens', '成本']}>
            {compareResults.map((r) => (
              <tr key={r.model}>
                <td className="mono">{r.model}</td>
                <td className="dim" style={{ maxWidth: 380 }}>
                  {r.error ? <span style={{ color: 'var(--err)' }}>{r.error}</span> : r.content.slice(0, 140)}
                </td>
                <td>{r.latencyMs ?? '-'} ms</td>
                <td>{r.tokens ?? '-'}</td>
                <td>{formatCost(r.costMicrounits)}</td>
              </tr>
            ))}
          </Table>
        </Card>
      )}
    </>
  )
}
