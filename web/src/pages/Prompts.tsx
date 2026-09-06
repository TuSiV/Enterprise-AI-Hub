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
    if (tone !== 'err') setTimeout(() => setToast({ message: '', tone }), 2600)
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
      notify(t("新版本已创建（draft）"))
      load()
    } catch (e: any) {
      notify(e.message, 'err')
    }
  }

  const publish = async (v: PromptVersion) => {
    try {
      await api.post(`/api/v1/admin/prompt-versions/${v.id}/publish`)
      notify(t("v{0} 已发布（旧版本自动废弃）", [v.version]))
      load()
    } catch (e: any) {
      notify(e.message, 'err')
    }
  }

  const deprecate = async (v: PromptVersion) => {
    try {
      await api.post(`/api/v1/admin/prompt-versions/${v.id}/deprecate`)
      notify(t("v{0} 已废弃", [v.version]))
      load()
    } catch (e: any) {
      notify(e.message, 'err')
    }
  }

  const remove = async (p: PromptDetail) => {
    if (!confirm(t("删除 Prompt「{0}」及全部版本？", [p.key]))) return
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
          <h2>{t('提示词')}</h2>
          <div className="page-sub">{t("版本化管理：draft → published → deprecated；Published 不可原地修改")}</div>
        </div>
        <Button variant="primary" onClick={() => setCreating(true)}>
          {t("+ 新建 Prompt")}</Button>
      </div>

      <Card>
        {prompts.length ? (
          <Table head={['Key', t("名称"), t("版本数"), 'Published', t("操作")]}>
            {prompts.map((p) => (
              <tr key={p.id}>
                <td className="mono">{p.key}</td>
                <td>
                  {p.name}
                  {p.description && <div className="dim">{p.description}</div>}
                </td>
                <td>{p.versions.length}</td>
                <td>{p.publishedVersion ? <Badge tone="ok">v{p.publishedVersion}</Badge> : <Badge tone="muted">{t("未发布")}</Badge>}</td>
                <td>
                  <Button variant="ghost" onClick={() => setEditing(p)}>
                    {t("版本管理")}</Button>
                  <Button variant="ghost" onClick={() => remove(p)}>
                    {t("删除")}</Button>
                </td>
              </tr>
            ))}
          </Table>
        ) : (
          <EmptyState title={t("暂无 Prompt")} />
        )}
      </Card>

      {creating && (
        <PromptForm
          onClose={() => setCreating(false)}
          onSaved={() => {
            setCreating(false)
            notify(t("已创建"))
            load()
          }}
        />
      )}
      {editing && (
        <Modal title={t("版本管理 · {0}", [editing.key])} onClose={() => setEditing(null)} wide>
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
      <Card title={t("新版本草稿")}>
        <Field label={t("System 模板（支持 {{变量}}）")}>
          <textarea rows={3} value={system} onChange={(e) => setSystem(e.target.value)} />
        </Field>
        <Field label={t("User 模板（支持 {{变量}}）")}>
          <textarea rows={3} value={user} onChange={(e) => setUser(e.target.value)} />
        </Field>
        <Button variant="primary" onClick={() => onCreate(prompt, system, user)} disabled={!system && !user}>
          {t("创建新版本")}</Button>
      </Card>
      <Table head={[t("版本"), t("状态"), t("System 预览"), t("操作")]}>
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
                  {t("发布")}</Button>
              )}
              {v.status === 'published' && (
                <Button variant="ghost" onClick={() => onDeprecate(v)}>
                  {t("废弃")}</Button>
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
    <Modal title={t("新建 Prompt")} onClose={onClose}>
      <Field label="Key">
        <input value={key} onChange={(e) => setKey(e.target.value)} placeholder="legal-summary" />
      </Field>
      <Field label={t("名称")}>
        <input value={name} onChange={(e) => setName(e.target.value)} />
      </Field>
      <Field label={t("描述")}>
        <input value={description} onChange={(e) => setDescription(e.target.value)} />
      </Field>
      {error && <p style={{ color: 'var(--err)', fontSize: 12.5 }}>{error}</p>}
      <div style={{ display: 'flex', justifyContent: 'flex-end' }}>
        <Button variant="primary" onClick={save} disabled={!key || !name}>
          {t("创建")}</Button>
      </div>
    </Modal>
  )
}
