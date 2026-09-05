import { useEffect, useState } from 'react'
import { api, qs } from '../api/client'
import type { AuditEventDto } from '../api/types'
import { formatTime } from '../api/types'
import { Badge, Button, Card, Table, Spinner, EmptyState } from '../components/ui'

export default function Audit() {
  const [items, setItems] = useState<AuditEventDto[]>([])
  const [total, setTotal] = useState(0)
  const [page, setPage] = useState(1)
  const [eventType, setEventType] = useState('')
  const [loading, setLoading] = useState(true)
  const pageSize = 50

  useEffect(() => {
    api
      .getEnvelope<AuditEventDto[]>(`/api/v1/admin/audit${qs({ page, pageSize, eventType })}`)
      .then((resp) => {
        setItems(resp.data ?? [])
        setTotal(resp.meta?.total ?? 0)
      })
      .finally(() => setLoading(false))
  }, [page, eventType])

  const pages = Math.max(1, Math.ceil(total / pageSize))

  return (
    <>
      <div className="page-head">
        <div>
          <h2>审计</h2>
          <div className="page-sub">管理操作与请求级审计事件（默认 Metadata Only，内容按策略保存）</div>
        </div>
      </div>
      <Card>
        <div className="filters">
          <select value={eventType} onChange={(e) => { setEventType(e.target.value); setPage(1) }}>
            <option value="">全部事件</option>
            {['request.completed', 'request.failed', 'request.client_cancelled', 'provider.created', 'provider.updated', 'provider.deleted', 'provider.tested', 'provider.models_discovered', 'model.created', 'virtual_model.created', 'virtual_model.updated', 'application.created', 'application.updated', 'api_key.created', 'api_key.revoked'].map((t) => (
              <option key={t}>{t}</option>
            ))}
          </select>
        </div>
        {loading ? (
          <Spinner />
        ) : items.length ? (
          <>
            <Table head={['时间', '事件', '资源', '决策', 'Trace', '元数据']}>
              {items.map((e) => (
                <tr key={e.id}>
                  <td className="dim">{formatTime(e.createdAt)}</td>
                  <td>
                    <Badge tone={e.eventType.includes('failed') || e.eventType.includes('revoked') || e.eventType.includes('deleted') ? 'err' : e.eventType.startsWith('request') ? 'info' : 'muted'}>
                      {e.eventType}
                    </Badge>
                  </td>
                  <td className="mono dim">
                    {e.resourceType ? `${e.resourceType}${e.resourceId ? `:${e.resourceId.slice(0, 8)}` : ''}` : '-'}
                  </td>
                  <td className="dim">{e.decision ?? '-'}</td>
                  <td className="mono dim">{e.traceId ? e.traceId.slice(0, 8) : '-'}</td>
                  <td className="mono dim" style={{ maxWidth: 320, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                    {JSON.stringify(e.metadata)}
                  </td>
                </tr>
              ))}
            </Table>
            <div className="pagination">
              <span>
                共 {total} 条 · 第 {page}/{pages} 页
              </span>
              <Button variant="ghost" disabled={page <= 1} onClick={() => setPage(page - 1)}>
                上一页
              </Button>
              <Button variant="ghost" disabled={page >= pages} onClick={() => setPage(page + 1)}>
                下一页
              </Button>
            </div>
          </>
        ) : (
          <EmptyState title="暂无审计事件" />
        )}
      </Card>
    </>
  )
}
