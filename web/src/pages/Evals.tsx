import { t } from '../i18n'
import { useEffect, useState } from 'react'
import { api } from '../api/client'
import { formatCost } from '../api/types'
import { Badge, Button, Card, EmptyState, Field, Modal, Table, Toast } from '../components/ui'

interface Dataset {
  id: string
  key: string
  name: string
  description: string | null
}

interface EvalRun {
  id: string
  label: string
  status: string
  startedAt: string
  summary: { cases?: number; avgScore?: number; avgLatencyMs?: number; totalCostMicrounits?: number }
}

interface RunOutcome {
  runId: string
  summary: any
  results: {
    caseId: string
    responseText: string | null
    latencyMs: number | null
    costMicrounits: number
    score: { exact?: boolean; contains?: boolean; score?: number }
    judge: { score?: number; reason?: string } | null
  }[]
}

export default function Evals() {
  const [datasets, setDatasets] = useState<Dataset[]>([])
  const [selected, setSelected] = useState<Dataset | null>(null)
  const [cases, setCases] = useState<any[]>([])
  const [runs, setRuns] = useState<EvalRun[]>([])
  const [outcome, setOutcome] = useState<RunOutcome | null>(null)
  const [creating, setCreating] = useState(false)
  const [model, setModel] = useState('general-smart')
  const [toast, setToast] = useState<{ message: string; tone: 'ok' | 'err' }>({ message: '', tone: 'ok' })

  const notify = (message: string, tone: 'ok' | 'err' = 'ok') => {
    setToast({ message, tone })
    if (tone !== 'err') setTimeout(() => setToast({ message: '', tone }), 3000)
  }

  const load = () => api.get<Dataset[]>('/api/v1/admin/evals/datasets').then(setDatasets)
  useEffect(() => {
    load().catch((e) => notify(e.message, 'err'))
  }, [])

  const open = async (d: Dataset) => {
    setSelected(d)
    const detail = await api.get<any>(`/api/v1/admin/evals/datasets/${d.id}`)
    setCases(detail.cases ?? [])
    setRuns(detail.runs ?? [])
    setOutcome(null)
  }

  const addCase = async (question: string, expected: string) => {
    if (!selected) return
    try {
      await api.post(`/api/v1/admin/evals/datasets/${selected.id}/cases`, {
        caseName: `case-${cases.length + 1}`,
        input: { question },
        expectedOutput: expected || undefined,
      })
      await open(selected)
    } catch (e: any) {
      notify(e.message, 'err')
    }
  }

  const run = async () => {
    if (!selected) return
    try {
      const r = await api.post<RunOutcome>(`/api/v1/admin/evals/datasets/${selected.id}/runs`, {
        label: `run-${new Date().toISOString().slice(11, 19)}`,
        candidate: { model },
        judgeConfig: {},
      })
      setOutcome(r)
      await open(selected)
    } catch (e: any) {
      notify(e.message, 'err')
    }
  }

  return (
    <>
      <div className="page-head">
        <div>
          <h2>{t("评测")}</h2>
          <div className="page-sub">{t("Dataset/Cases → 多候选运行 → rule 分 + 可选 LLM Judge + 成本/延迟对比")}</div>
        </div>
        <Button variant="primary" onClick={() => setCreating(true)}>
          {t("+ 新建数据集")}</Button>
      </div>

      <Card>
        {datasets.length ? (
          <Table head={['Key', t("名称"), t("操作")]}>
            {datasets.map((d) => (
              <tr key={d.id}>
                <td className="mono">{d.key}</td>
                <td>{d.name}</td>
                <td>
                  <Button variant="ghost" onClick={() => open(d)}>
                    {t("打开")}</Button>
                </td>
              </tr>
            ))}
          </Table>
        ) : (
          <EmptyState title={t("暂无数据集")} />
        )}
      </Card>

      {selected && (
        <>
          <Card title={t("用例 · {0}（{1}）", [selected.key, cases.length])}>
            {cases.length > 0 && (
              <Table head={[t("名称"), t("问题"), t("期望输出")]}>
                {cases.map((c) => (
                  <tr key={c.id}>
                    <td>{c.name}</td>
                    <td className="mono dim">{JSON.stringify(c.input)}</td>
                    <td>{c.expectedOutput ?? '-'}</td>
                  </tr>
                ))}
              </Table>
            )}
            <CaseEditor onAdd={addCase} />
          </Card>

          <Card title={t("运行评测")}>
            <Field label={t("候选模型")}>
              <input value={model} onChange={(e) => setModel(e.target.value)} />
            </Field>
            <Button variant="primary" onClick={run} disabled={!cases.length}>
              {t("运行（对全部用例）")}</Button>
            {runs.length > 0 && (
              <div style={{ marginTop: 12 }}>
                <Table head={['Run', t("状态"), t("平均分"), t("平均延迟"), t("总成本")]}>
                  {runs.map((r) => (
                    <tr key={r.id}>
                      <td className="mono">{r.label}</td>
                      <td>
                        <Badge tone={r.status === 'completed' ? 'ok' : 'err'}>{r.status}</Badge>
                      </td>
                      <td>{r.summary?.avgScore ?? '-'}</td>
                      <td>{r.summary?.avgLatencyMs ?? '-'} ms</td>
                      <td>{formatCost(r.summary?.totalCostMicrounits ?? 0)}</td>
                    </tr>
                  ))}
                </Table>
              </div>
            )}
          </Card>

          {outcome && (
            <Card title={t("最近一次运行结果")}>
              <Table head={[t("回答"), t("得分"), t("延迟"), t("成本")]}>
                {outcome.results.map((r, i) => (
                  <tr key={i}>
                    <td className="dim" style={{ maxWidth: 380 }}>
                      {r.responseText?.slice(0, 120)}
                    </td>
                    <td>
                      <Badge tone={(r.score?.score ?? 0) >= 0.7 ? 'ok' : 'warn'}>
                        {r.score?.score ?? 0}
                      </Badge>
                    </td>
                    <td>{r.latencyMs ?? '-'} ms</td>
                    <td>{formatCost(r.costMicrounits)}</td>
                  </tr>
                ))}
              </Table>
            </Card>
          )}
        </>
      )}

      {creating && (
        <Modal title={t("新建数据集")} onClose={() => setCreating(false)}>
          <DatasetForm
            onSaved={() => {
              setCreating(false)
              notify(t("已创建"))
              load()
            }}
          />
        </Modal>
      )}
      <Toast message={toast.message} tone={toast.tone} />
    </>
  )
}

function CaseEditor({ onAdd }: { onAdd: (q: string, e: string) => void }) {
  const [q, setQ] = useState('')
  const [e, setE] = useState('')
  return (
    <div style={{ marginTop: 12 }}>
      <div className="field-row">
        <Field label={t("问题")}>
          <input value={q} onChange={(ev) => setQ(ev.target.value)} />
        </Field>
        <Field label={t("期望输出（可选）")}>
          <input value={e} onChange={(ev) => setE(ev.target.value)} />
        </Field>
      </div>
      <Button onClick={() => { onAdd(q, e); setQ(''); setE('') }} disabled={!q}>
        {t("添加用例")}</Button>
    </div>
  )
}

function DatasetForm({ onSaved }: { onSaved: () => void }) {
  const [key, setKey] = useState('')
  const [name, setName] = useState('')
  const [error, setError] = useState('')
  const save = async () => {
    try {
      await api.post('/api/v1/admin/evals/datasets', { key, name })
      onSaved()
    } catch (e: any) {
      setError(e.message)
    }
  }
  return (
    <>
      <Field label="Key">
        <input value={key} onChange={(e) => setKey(e.target.value)} placeholder="smoke-eval" />
      </Field>
      <Field label={t("名称")}>
        <input value={name} onChange={(e) => setName(e.target.value)} />
      </Field>
      {error && <p style={{ color: 'var(--err)', fontSize: 12.5 }}>{error}</p>}
      <div style={{ display: 'flex', justifyContent: 'flex-end' }}>
        <Button variant="primary" onClick={save} disabled={!key || !name}>
          {t("创建")}</Button>
      </div>
    </>
  )
}
