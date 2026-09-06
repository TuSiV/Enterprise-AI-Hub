import { useEffect, useState } from 'react'
import { api } from '../api/client'
import { Badge, Button, Card, EmptyState, Field, Modal, Table, Toast } from '../components/ui'

interface Kb {
  id: string
  key: string
  name: string
  visibility: string
  embeddingModelId: string | null
  documentCount: number
  chunkCount: number
}

interface Doc {
  id: string
  filename: string
  mime_type: string
  size_bytes: number
  parse_status: string
  index_status: string
}

export default function Knowledge() {
  const [kbs, setKbs] = useState<Kb[]>([])
  const [models, setModels] = useState<{ id: string; label: string }[]>([])
  const [creating, setCreating] = useState(false)
  const [selected, setSelected] = useState<Kb | null>(null)
  const [docs, setDocs] = useState<Doc[]>([])
  const [queryResult, setQueryResult] = useState<{ hits: any[]; context: string } | null>(null)
  const [uploadContent, setUploadContent] = useState('')
  const [queryText, setQueryText] = useState('')
  const [toast, setToast] = useState<{ message: string; tone: 'ok' | 'err' }>({ message: '', tone: 'ok' })

  const notify = (message: string, tone: 'ok' | 'err' = 'ok') => {
    setToast({ message, tone })
    setTimeout(() => setToast({ message: '', tone }), 3000)
  }

  const load = () =>
    Promise.all([api.get<Kb[]>('/api/v1/admin/knowledge-bases'), api.get<any[]>('/api/v1/admin/models')]).then(
      ([k, m]) => {
        setKbs(k)
        setModels(
          m.map((x) => ({ id: x.id, label: `${x.modelKey}（${x.providerName ?? ''}）` })),
        )
      },
    )
  useEffect(() => {
    load().catch((e) => notify(e.message, 'err'))
  }, [])

  const openKb = async (kb: Kb) => {
    setSelected(kb)
    setDocs(await api.get<Doc[]>(`/api/v1/admin/knowledge-bases/${kb.id}/documents`))
    setQueryResult(null)
  }

  const upload = async () => {
    if (!selected || !uploadContent.trim()) return
    try {
      await api.post(`/api/v1/admin/documents/${selected.id}/upload-content`, {
        filename: `note-${Date.now()}.txt`,
        mimeType: 'text/plain',
        content: uploadContent,
      })
      setUploadContent('')
      notify('文档已索引（ready）')
      await openKb(selected)
      load()
    } catch (e: any) {
      notify(e.message, 'err')
    }
  }

  const query = async () => {
    if (!selected || !queryText.trim()) return
    try {
      const r = await api.post<{ hits: any[]; context: string }>(
        `/api/v1/admin/knowledge-bases/${selected.id}/query`,
        { query: queryText, topK: 5 },
      )
      setQueryResult(r)
    } catch (e: any) {
      notify(e.message, 'err')
    }
  }

  const removeKb = async (kb: Kb) => {
    if (!confirm(`删除知识库「${kb.key}」及全部文档？`)) return
    try {
      await api.del(`/api/v1/admin/knowledge-bases/${kb.id}`)
      setSelected(null)
      load()
    } catch (e: any) {
      notify(e.message, 'err')
    }
  }

  const bindModel = async (kb: Kb, modelId: string) => {
    try {
      await api.post(`/api/v1/admin/knowledge-bases/${kb.id}/bind-embedding-model`, { modelId })
      notify('Embedding 模型已绑定')
      load()
    } catch (e: any) {
      notify(e.message, 'err')
    }
  }

  return (
    <>
      <div className="page-head">
        <div>
          <h2>知识库</h2>
          <div className="page-sub">上传 → 解析 → Chunk → Embedding → 检索（引用保留 filename/page，§19.5）</div>
        </div>
        <Button variant="primary" onClick={() => setCreating(true)}>
          + 新建知识库
        </Button>
      </div>

      <Card>
        {kbs.length ? (
          <Table head={['Key', '名称', '可见性', '文档', 'Chunks', 'Embedding', '操作']}>
            {kbs.map((kb) => (
              <tr key={kb.id}>
                <td className="mono">{kb.key}</td>
                <td>{kb.name}</td>
                <td>
                  <Badge tone={kb.visibility === 'public' ? 'ok' : 'muted'}>{kb.visibility}</Badge>
                </td>
                <td>{kb.documentCount}</td>
                <td>{kb.chunkCount}</td>
                <td>{kb.embeddingModelId ? <Badge tone="info">已绑定</Badge> : <Badge tone="warn">未绑定</Badge>}</td>
                <td>
                  <Button variant="ghost" onClick={() => openKb(kb)}>
                    打开
                  </Button>
                  {!kb.embeddingModelId && (
                    <select
                      defaultValue=""
                      onChange={(e) => e.target.value && bindModel(kb, e.target.value)}
                      style={{ border: '1px solid var(--border)', borderRadius: 8, padding: '4px 6px', fontSize: 12 }}
                    >
                      <option value="">绑定 Embedding…</option>
                      {models.map((m) => (
                        <option key={m.id} value={m.id}>
                          {m.label}
                        </option>
                      ))}
                    </select>
                  )}
                  <Button variant="ghost" onClick={() => removeKb(kb)}>
                    删除
                  </Button>
                </td>
              </tr>
            ))}
          </Table>
        ) : (
          <EmptyState title="暂无知识库" hint="先在 Provider 页发现一个 embedding 模型，再建库绑定" />
        )}
      </Card>

      {selected && (
        <>
          <Card title={`文档 · ${selected.key}`}>
            {docs.length ? (
              <Table head={['文件名', '类型', '大小', '解析', '索引']}>
                {docs.map((d) => (
                  <tr key={d.id}>
                    <td className="mono">{d.filename}</td>
                    <td>{d.mime_type}</td>
                    <td>{(d.size_bytes / 1024).toFixed(1)} KB</td>
                    <td>
                      <Badge tone={d.parse_status === 'ready' ? 'ok' : d.parse_status === 'parse_failed' ? 'err' : 'muted'}>
                        {d.parse_status}
                      </Badge>
                    </td>
                    <td>
                      <Badge tone={d.index_status === 'ready' ? 'ok' : d.index_status === 'index_failed' ? 'err' : 'muted'}>
                        {d.index_status}
                      </Badge>
                    </td>
                  </tr>
                ))}
              </Table>
            ) : (
              <EmptyState title="暂无文档" />
            )}
            <Field label="粘贴文本内容并索引（txt）" hint="PDF/DOCX 上传需启用 Python Runtime（M11）">
              <textarea rows={4} value={uploadContent} onChange={(e) => setUploadContent(e.target.value)} />
            </Field>
            <Button variant="primary" onClick={upload} disabled={!uploadContent.trim()}>
              上传并索引
            </Button>
          </Card>

          <Card title="检索测试">
            <Field label="查询">
              <input value={queryText} onChange={(e) => setQueryText(e.target.value)} placeholder="例如：北京办公室" />
            </Field>
            <Button onClick={query} disabled={!queryText.trim()}>
              检索
            </Button>
            {queryResult && (
              <div style={{ marginTop: 12 }}>
                <Table head={['#', '分数', '文件', '内容预览']}>
                  {queryResult.hits.map((h, i) => (
                    <tr key={h.chunkId}>
                      <td>[{i + 1}]</td>
                      <td>{h.score.toFixed(4)}</td>
                      <td className="mono">{h.filename}</td>
                      <td className="dim" style={{ maxWidth: 420 }}>
                        {h.content.slice(0, 160)}
                      </td>
                    </tr>
                  ))}
                </Table>
              </div>
            )}
          </Card>
        </>
      )}

      {creating && (
        <KbForm
          onClose={() => setCreating(false)}
          onSaved={() => {
            setCreating(false)
            notify('已创建，请绑定 Embedding 模型')
            load()
          }}
        />
      )}
      <Toast message={toast.message} tone={toast.tone} />
    </>
  )
}

function KbForm({ onClose, onSaved }: { onClose: () => void; onSaved: () => void }) {
  const [key, setKey] = useState('')
  const [name, setName] = useState('')
  const [visibility, setVisibility] = useState('private')
  const [error, setError] = useState('')
  const save = async () => {
    try {
      await api.post('/api/v1/admin/knowledge-bases', { key, name, visibility })
      onSaved()
    } catch (e: any) {
      setError(e.message)
    }
  }
  return (
    <Modal title="新建知识库" onClose={onClose}>
      <Field label="Key">
        <input value={key} onChange={(e) => setKey(e.target.value)} placeholder="legal-kb" />
      </Field>
      <Field label="名称">
        <input value={name} onChange={(e) => setName(e.target.value)} />
      </Field>
      <Field label="可见性">
        <select value={visibility} onChange={(e) => setVisibility(e.target.value)}>
          <option value="private">private</option>
          <option value="department">department</option>
          <option value="public">public</option>
        </select>
      </Field>
      {error && <p style={{ color: 'var(--err)', fontSize: 12.5 }}>{error}</p>}
      <div style={{ display: 'flex', justifyContent: 'flex-end' }}>
        <Button variant="primary" onClick={save} disabled={!key || !name}>
          创建
        </Button>
      </div>
    </Modal>
  )
}
