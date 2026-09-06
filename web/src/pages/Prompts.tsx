import { useEffect, useState } from 'react'
import { api } from '../api/client'
import { Badge, Button, Card, EmptyState, Field, Modal, Table, Toast } from '../components/ui'

interface PromptVersion {
  id: string
  version: number
  status: string
  systemTemplate?: string | null
  userTemplate?: string | null
  createdAt: string
}

interface PromptDetail {
  id: string
  key: string
  name: string
  description: string | null
  versions: PromptVersion[]
  publishedVersion: number | null
}

export default function Prompts() {
  const [prompts, setPrompts] = useState<PromptDetail[]>([])
  const [creating, setCreating] = useState(false)
  const [editing, setEditing] = useState<PromptDetail | null>(null)
  const [toast, setToast] = useState<{ message: string; tone: 'ok' | 'err' }>({ message: '', tone: 'ok' })

  const notify = (message: string, tone: 'ok' | 'err' = 'ok') => {
    setToast({ message, tone })
    setTimeout(() => setToast({ message: '', tone }), 2600)
  }

  const load = () => api.get<PromptDetail[]>('/api/v1/admin/prompts').then(setPrompts)
  useEffect(() => {
    load().catch((e) => notify(e.message, 'err'))
  }, [])

  const createVersion = async (p: PromptDetail, system: string, user: string) => {
    try {
      await api.post(`/api/v1/admin/prompts/${p.id}/versions`, {
        systemTemplate: system || undefined,
        userTemplate: user || undefined,
      })
      notify('新版本已创建（draft）')
      load()
    } catch (e: any) {
      notify(e.message, 'err')
    }
  }

  const publish = async (v: PromptVersion) => {
    try {
      await api.post(`/api/v1/admin/prompt-versions/${v.id}/publish`)
      notify(`v${v.version} 已发布（旧版本自动废弃）`)
      load()
    } catch (e: any) {
      notify(e.message, 'err')
    }
  }

  const deprecate = async (v: PromptVersion) => {
    try {
      await api.post(`/api/v1/admin/prompt-versions/${v.id}/deprecate`)
      notify(`v${v.version} 已废弃`)
      load()
    } catch (e: any) {
      notify(e.message, 'err')
    }
  }

  const remove = async (p: PromptDetail) => {
    if (!confirm(`删除 Prompt「${p.key}」及全部版本？`)) return
    try {
      await api.del(`/api/v1/admin/prompts/${p.id}`)
      load()
    } catch (e: any) {
      notify(e.message, 'err')
    }
  }

  return (
    <>
      <div className="page-head">
        <div>
          <h2>Prompts</h2>
          <div className="page-sub">版本化管理：draft → published → deprecated；Published 不可原地修改（§18.1）</div>
        </div>
        <Button variant="primary" onClick={() => setCreating(true)}>
          + 新建 Prompt
        </Button>
      </div>

      <Card>
        {prompts.length ? (
          <Table head={['Key', '名称', '版本数', 'Published', '操作']}>
            {prompts.map((p) => (
              <tr key={p.id}>
                <td className="mono">{p.key}</td>
                <td>
                  {p.name}
                  {p.description && <div className="dim">{p.description}</div>}
                </td>
                <td>{p.versions.length}</td>
                <td>{p.publishedVersion ? <Badge tone="ok">v{p.publishedVersion}</Badge> : <Badge tone="muted">未发布</Badge>}</td>
                <td>
                  <Button variant="ghost" onClick={() => setEditing(p)}>
                    版本管理
                  </Button>
                  <Button variant="ghost" onClick={() => remove(p)}>
                    删除
                  </Button>
                </td>
              </tr>
            ))}
          </Table>
        ) : (
          <EmptyState title="暂无 Prompt" />
        )}
      </Card>

      {creating && (
        <PromptForm
          onClose={() => setCreating(false)}
          onSaved={() => {
            setCreating(false)
            notify('已创建')
            load()
          }}
        />
      )}
      {editing && (
        <Modal title={`版本管理 · ${editing.key}`} onClose={() => setEditing(null)} wide>
          <VersionEditor prompt={editing} onCreate={createVersion} onPublish={publish} onDeprecate={deprecate} />
        </Modal>
      )}
      <Toast message={toast.message} tone={toast.tone} />
    </>
  )
}

function VersionEditor({
  prompt,
  onCreate,
  onPublish,
  onDeprecate,
}: {
  prompt: PromptDetail
  onCreate: (p: PromptDetail, system: string, user: string) => void
  onPublish: (v: PromptVersion) => void
  onDeprecate: (v: PromptVersion) => void
}) {
  const [system, setSystem] = useState('')
  const [user, setUser] = useState('')
  return (
    <>
      <Card title="新版本草稿">
        <Field label="System 模板（支持 {{变量}}）">
          <textarea rows={3} value={system} onChange={(e) => setSystem(e.target.value)} />
        </Field>
        <Field label="User 模板（支持 {{变量}}）">
          <textarea rows={3} value={user} onChange={(e) => setUser(e.target.value)} />
        </Field>
        <Button variant="primary" onClick={() => onCreate(prompt, system, user)} disabled={!system && !user}>
          创建新版本
        </Button>
      </Card>
      <Table head={['版本', '状态', 'System 预览', '操作']}>
        {prompt.versions.map((v) => (
          <tr key={v.id}>
            <td>v{v.version}</td>
            <td>
              <Badge tone={v.status === 'published' ? 'ok' : v.status === 'deprecated' ? 'err' : 'muted'}>{v.status}</Badge>
            </td>
            <td className="mono dim" style={{ maxWidth: 320, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
              {v.systemTemplate ?? '-'}
            </td>
            <td>
              {v.status !== 'published' && (
                <Button variant="ghost" onClick={() => onPublish(v)}>
                  发布
                </Button>
              )}
              {v.status === 'published' && (
                <Button variant="ghost" onClick={() => onDeprecate(v)}>
                  废弃
                </Button>
              )}
            </td>
          </tr>
        ))}
      </Table>
    </>
  )
}

function PromptForm({ onClose, onSaved }: { onClose: () => void; onSaved: () => void }) {
  const [key, setKey] = useState('')
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [error, setError] = useState('')
  const save = async () => {
    try {
      await api.post('/api/v1/admin/prompts', { key, name, description: description || undefined })
      onSaved()
    } catch (e: any) {
      setError(e.message)
    }
  }
  return (
    <Modal title="新建 Prompt" onClose={onClose}>
      <Field label="Key">
        <input value={key} onChange={(e) => setKey(e.target.value)} placeholder="legal-summary" />
      </Field>
      <Field label="名称">
        <input value={name} onChange={(e) => setName(e.target.value)} />
      </Field>
      <Field label="描述">
        <input value={description} onChange={(e) => setDescription(e.target.value)} />
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
