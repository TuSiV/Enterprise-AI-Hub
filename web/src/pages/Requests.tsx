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
import { api, qs } from '../api/client'
import type { RequestListItem, RequestDetail } from '../api/types'
import { formatCost, formatTime } from '../api/types'
import { Badge, Button, Card, Modal, Table, StatusBadge, Spinner, EmptyState, Toast } from '../components/ui'

export default function Requests() {
  const [items, setItems] = useState<RequestListItem[]>([])
  const [total, setTotal] = useState(0)
  const [page, setPage] = useState(1)
  const [status, setStatus] = useState('')
  const [model, setModel] = useState('')
  const [loading, setLoading] = useState(true)
  const [detail, setDetail] = useState<RequestDetail | null>(null)
  const [toast, setToast] = useState<{ message: string; tone: 'ok' | 'err' }>({ message: '', tone: 'ok' })
  const pageSize = 50

  const notify = (message: string, tone: 'ok' | 'err' = 'err') => {
    setToast({ message, tone })
    if (tone !== 'err') setTimeout(() => setToast({ message: '', tone }), 2600)
  }

  const load = () =>
    api.getEnvelope<RequestListItem[]>(`/api/v1/admin/requests${qs({ page, pageSize, status, model })}`).then((resp) => {
      setItems(resp.data ?? [])
      setTotal(resp.meta?.total ?? 0)
    })

  useEffect(() => {
    load().catch((e) => notify(e.message, 'err')).finally(() => setLoading(false))
  }, [page, status, model])

  const openDetail = async (id: string) => {
    try {
      setDetail(await api.get<RequestDetail>(`/api/v1/admin/requests/${id}`))
    } catch (e: any) {
      notify(e.message)
    }
  }

  const pages = Math.max(1, Math.ceil(total / pageSize))

  return (
    <>
      <div className="page-head">
        <div>
          <h2>{t("请求记录")}</h2>
          <div className="page-sub">{t("每次经过 Gateway 的 AI 请求：模型解析、延迟、Token 与成本")}</div>
        </div>
      </div>

      <Card>
        <div className="filters">
          <select value={status} onChange={(e) => { setStatus(e.target.value); setPage(1) }}>
            <option value="">{t("全部状态")}</option>
            {['completed', 'failed', 'client_cancelled', 'timeout', 'running'].map((s) => (
              <option key={s}>{s}</option>
            ))}
          </select>
          <input value={model} onChange={(e) => { setModel(e.target.value); setPage(1) }} placeholder={t("按模型过滤")} style={{ width: 200 }} />
        </div>
        {loading ? (
          <Spinner />
        ) : items.length ? (
          <>
            <Table head={[t("时间"), t("应用"), t("请求模型"), t("实际模型"), 'Provider', t("状态"), t("延迟"), 'TTFT', 'Tokens', t("成本"), t("错误")]}>
              {items.map((r) => (
                <tr key={r.id} className="clickable" onClick={() => openDetail(r.id)}>
                  <td className="dim">{formatTime(r.startedAt)}</td>
                  <td className="mono dim">{r.applicationKey ?? '-'}</td>
                  <td className="mono">{r.requestedModel}</td>
                  <td className="mono">{r.resolvedModelKey ?? '-'}</td>
                  <td className="mono dim">{r.providerKey ?? '-'}</td>
                  <td>
                    <StatusBadge status={r.status} />
                  </td>
                  <td>{r.latencyMs != null ? `${r.latencyMs} ms` : '-'}</td>
                  <td>{r.ttftMs != null ? `${r.ttftMs} ms` : '-'}</td>
                  <td>{r.totalTokens ?? '-'}</td>
                  <td>{formatCost(r.costMicrounits)}</td>
                  <td className="mono dim">{r.errorCode ?? ''}</td>
                </tr>
              ))}
            </Table>
            <div className="pagination">
              <span>
                {t('共 {0} 条 · 第 {1}/{2} 页', [total, page, pages])}</span>
              <Button variant="ghost" disabled={page <= 1} onClick={() => setPage(page - 1)}>
                {t("上一页")}</Button>
              <Button variant="ghost" disabled={page >= pages} onClick={() => setPage(page + 1)}>
                {t("下一页")}</Button>
            </div>
          </>
        ) : (
          <EmptyState title={t("暂无请求")} hint={t("通过 /v1/chat/completions 发起一次调用后在此查看")} />
        )}
      </Card>

      {detail && (
        <Modal title={t("请求详情")} onClose={() => setDetail(null)} wide>
          <Table head={[t("字段"), t("值")]}>
            <tr>
              <td className="dim">Request ID</td>
              <td className="mono">{detail.id}</td>
            </tr>
            <tr>
              <td className="dim">Trace ID</td>
              <td className="mono">{detail.traceId ?? '-'}</td>
            </tr>
            <tr>
              <td className="dim">{t("请求模型 → 实际模型")}</td>
              <td className="mono">
                {detail.requestedModel} → {detail.resolvedModelKey ?? '-'}
              </td>
            </tr>
            <tr>
              <td className="dim">{t("状态 / HTTP")}</td>
              <td>
                <StatusBadge status={detail.status} /> <span className="mono dim">{detail.httpStatus ?? ''}</span>
              </td>
            </tr>
            <tr>
              <td className="dim">{t("延迟 / TTFT / 重试")}</td>
              <td>
                {detail.latencyMs ?? '-'} ms / {detail.ttftMs ?? '-'} ms / {detail.retryCount}
              </td>
            </tr>
            {detail.errorMessage && (
              <tr>
                <td className="dim">{t("错误")}</td>
                <td className="mono">{detail.errorMessage}</td>
              </tr>
            )}
          </Table>
          {detail.usage && (
            <>
              <h4 style={{ margin: '14px 0 8px' }}>Usage</h4>
              <pre className="json">{JSON.stringify(detail.usage, null, 2)}</pre>
            </>
          )}
          {detail.cost && (
            <>
              <h4 style={{ margin: '14px 0 8px' }}>{t("Cost（含计费快照）")}</h4>
              <pre className="json">{JSON.stringify(detail.cost, null, 2)}</pre>
            </>
          )}
        </Modal>
      )}
      <Toast message={toast.message} tone={toast.tone} />
    </>
  )
}
