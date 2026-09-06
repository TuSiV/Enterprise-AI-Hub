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

import { t } from '../i18n'
import { useEffect, useState } from 'react'
import { api } from '../api/client'
import { Badge, Button, Card, EmptyState, Field, Modal, Table, Toast } from '../components/ui'

interface Agent {
  id: string
  key: string
  name: string
  description: string | null
  status: string
}

interface AgentVersion {
  id: string
  version: number
  status: string
  modelRef: string
  maxSteps: number
  allowedTools: string[]
}

interface RunResult {
  runId: string
  status: string
  steps: number
  output: string | null
  toolCalls: { toolKey: string; status: string; latencyMs: number | null }[]
  error: string | null
  costMicrounits: number
}

export default function Agents() {
  const [agents, setAgents] = useState<Agent[]>([])
  const [tools, setTools] = useState<any[]>([])
  const [versions, setVersions] = useState<Record<string, AgentVersion[]>>({})
  const [creating, setCreating] = useState(false)
  const [runTarget, setRunTarget] = useState<Agent | null>(null)
  const [runResult, setRunResult] = useState<RunResult | null>(null)
  const [toast, setToast] = useState<{ message: string; tone: 'ok' | 'err' }>({ message: '', tone: 'ok' })

  const notify = (message: string, tone: 'ok' | 'err' = 'ok') => {
    setToast({ message, tone })
    if (tone !== 'err') setTimeout(() => setToast({ message: '', tone }), 3000)
  }

  const load = async () => {
    const [a, t] = await Promise.all([
      api.get<Agent[]>('/api/v1/admin/agents'),
      api.get<any[]>('/api/v1/admin/tools'),
    ])
    setAgents(a)
    setTools(t)
    const vs: Record<string, AgentVersion[]> = {}
    for (const agent of a) {
      vs[agent.id] = await api.get<AgentVersion[]>(`/api/v1/admin/agents/${agent.id}/versions`)
    }
    setVersions(vs)
  }
  useEffect(() => {
    load().catch((e) => notify(e.message, 'err'))
  }, [])

  const publish = async (v: AgentVersion) => {
    try {
      await api.post(`/api/v1/admin/agent-versions/${v.id}/publish`)
      notify(t("v{0} 已发布", [v.version]))
      load()
    } catch (e: any) {
      notify(e.message, 'err')
    }
  }

  const run = async (agent: Agent, task: string) => {
    try {
      const r = await api.post<RunResult>(`/api/v1/admin/agents/${agent.id}/run`, { input: { task } })
      setRunResult(r)
      load()
    } catch (e: any) {
      notify(e.message, 'err')
    }
  }

  return (
    <>
      <div className="page-head">
        <div>
          <h2>{t('智能体')}</h2>
          <div className="page-sub">{t("LLM + Tool 循环执行；maxSteps/工具白名单/全量 ToolCall 审计")}</div>
        </div>
        <Button variant="primary" onClick={() => setCreating(true)}>
          {t("+ 创建 Agent")}</Button>
      </div>

      <Card title={t("内置与已注册工具")}>
        {tools.length ? (
          <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
            {tools.map((t) => (
              <Badge key={t.id} tone={t.enabled ? 'info' : 'muted'}>
                {t.key}（{t.kind}）
              </Badge>
            ))}
          </div>
        ) : (
          <EmptyState title={t("暂无工具")} />
        )}
      </Card>

      <Card>
        {agents.length ? (
          <Table head={['Key', t("名称"), t("版本"), t("操作")]}>
            {agents.map((a) => (
              <tr key={a.id}>
                <td className="mono">{a.key}</td>
                <td>{a.name}</td>
                <td>
                  {(versions[a.id] ?? []).map((v) => (
                    <span key={v.id} style={{ marginRight: 6 }}>
                      <Badge tone={v.status === 'published' ? 'ok' : 'muted'}>
                        v{v.version}·{v.status}
                      </Badge>
                      {v.status !== 'published' && (
                        <button className="btn btn-ghost btn-sm" onClick={() => publish(v)}>
                          {t("发布")}</button>
                      )}
                    </span>
                  ))}
                </td>
                <td>
                  <Button variant="ghost" onClick={() => { setRunTarget(a); setRunResult(null) }}>
                    {t("运行测试")}</Button>
                </td>
              </tr>
            ))}
          </Table>
        ) : (
          <EmptyState title={t("暂无 Agent")} hint={t("创建后需发布版本才能运行")} />
        )}
      </Card>

      {runTarget && (
        <Modal title={t("运行 · {0}", [runTarget.key])} onClose={() => setRunTarget(null)} wide>
          <RunPanel agent={runTarget} onRun={run} result={runResult} />
        </Modal>
      )}
      {creating && (
        <AgentForm
          onClose={() => setCreating(false)}
          onSaved={() => {
            setCreating(false)
            notify(t("已创建（draft），请发布后运行"))
            load()
          }}
        />
      )}
      <Toast message={toast.message} tone={toast.tone} />
    </>
  )
}

function RunPanel({
  agent,
  onRun,
  result,
}: {
  agent: Agent
  onRun: (a: Agent, task: string) => void
  result: RunResult | null
}) {
  const [task, setTask] = useState(t("请做一个自我介绍"))
  return (
    <>
      <Field label={t("任务输入")}>
        <textarea rows={3} value={task} onChange={(e) => setTask(e.target.value)} />
      </Field>
      <Button variant="primary" onClick={() => onRun(agent, task)}>
        {t("运行（Published 版本）")}</Button>
      {result && (
        <div style={{ marginTop: 14 }}>
          <p>
            {t("状态：")}<Badge tone={result.status === 'completed' ? 'ok' : 'err'}>{result.status}</Badge> {t("· 步数")}{result.steps} ·{' '}
            {t("成本")}{result.costMicrounits} microunits
          </p>
          {result.output && <pre className="json">{result.output}</pre>}
          {result.error && <p style={{ color: 'var(--err)' }}>{result.error}</p>}
          {result.toolCalls.length > 0 && (
            <Table head={[t("工具"), t("状态"), t("延迟")]}>
              {result.toolCalls.map((c, i) => (
                <tr key={i}>
                  <td className="mono">{c.toolKey}</td>
                  <td>
                    <Badge tone={c.status === 'succeeded' ? 'ok' : 'err'}>{c.status}</Badge>
                  </td>
                  <td>{c.latencyMs ?? '-'} ms</td>
                </tr>
              ))}
            </Table>
          )}
        </div>
      )}
    </>
  )
}

function AgentForm({ onClose, onSaved }: { onClose: () => void; onSaved: () => void }) {
  const [key, setKey] = useState('')
  const [name, setName] = useState('')
  const [modelRef, setModelRef] = useState('general-smart')
  const [systemPrompt, setSystemPrompt] = useState(t("你是一个企业助手，需要时调用工具。"))
  const [allowedTools, setAllowedTools] = useState<string[]>([])
  const [allTools, setAllTools] = useState<any[]>([])
  const [error, setError] = useState('')
  useEffect(() => {
    api.get<any[]>('/api/v1/admin/tools').then(setAllTools).catch(() => {})
  }, [])
  const save = async () => {
    try {
      await api.post('/api/v1/admin/agents', {
        key,
        name,
        description: null,
        modelRef,
        systemPrompt,
        maxSteps: 6,
        allowedTools,
      })
      onSaved()
    } catch (e: any) {
      setError(e.message)
    }
  }
  return (
    <Modal title={t("创建 Agent（draft v1）")} onClose={onClose} wide>
      <Field label="Key">
        <input value={key} onChange={(e) => setKey(e.target.value)} placeholder="assistant" />
      </Field>
      <Field label={t("名称")}>
        <input value={name} onChange={(e) => setName(e.target.value)} />
      </Field>
      <Field label={t("模型（Virtual/Physical key）")}>
        <input value={modelRef} onChange={(e) => setModelRef(e.target.value)} />
      </Field>
      <Field label="System Prompt">
        <textarea rows={3} value={systemPrompt} onChange={(e) => setSystemPrompt(e.target.value)} />
      </Field>
      <Field label={t("允许的工具（不选 = 仅模型对话）")}>
        <div style={{ display: 'flex', gap: 10, flexWrap: 'wrap' }}>
          {allTools.map((t) => (
            <label key={t.id} style={{ fontSize: 13 }}>
              <input
                type="checkbox"
                checked={allowedTools.includes(t.key)}
                onChange={() =>
                  setAllowedTools(
                    allowedTools.includes(t.key)
                      ? allowedTools.filter((k) => k !== t.key)
                      : [...allowedTools, t.key],
                  )
                }
              />{' '}
              {t.key}
            </label>
          ))}
        </div>
      </Field>
      {error && <p style={{ color: 'var(--err)', fontSize: 12.5 }}>{error}</p>}
      <div style={{ display: 'flex', justifyContent: 'flex-end' }}>
        <Button variant="primary" onClick={save} disabled={!key || !name}>
          {t("创建")}</Button>
      </div>
    </Modal>
  )
}
