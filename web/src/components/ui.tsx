import { t } from '../i18n'
import React, { useEffect, useRef, useId, useState } from 'react'

export function Spinner({ label }: { label?: string }) {
  return (
    <div className="spinner-wrap">
      <div className="spinner" />
      {label && <span>{label}</span>}
    </div>
  )
}

export function Card({ title, children, actions }: { title?: string; children: React.ReactNode; actions?: React.ReactNode }) {
  return (
    <div className="card">
      {(title || actions) && (
        <div className="card-head">
          {title && <h3>{title}</h3>}
          {actions && <div className="card-actions">{actions}</div>}
        </div>
      )}
      {children}
    </div>
  )
}

export function Stat({ label, value, sub }: { label: string; value: React.ReactNode; sub?: string }) {
  return (
    <div className="stat">
      <div className="stat-label">{label}</div>
      <div className="stat-value">{value}</div>
      {sub && <div className="stat-sub">{sub}</div>}
    </div>
  )
}

type BadgeTone = 'ok' | 'warn' | 'err' | 'muted' | 'info'

export function Badge({ tone = 'muted', children }: { tone?: BadgeTone; children: React.ReactNode }) {
  return <span className={`badge badge-${tone}`}>{children}</span>
}

export function HealthBadge({ health }: { health: string }) {
  const tone = health === 'healthy' ? 'ok' : health === 'degraded' ? 'warn' : health === 'unavailable' ? 'err' : 'muted'
  const label = { healthy: t("健康"), degraded: t("降级"), unavailable: t("不可用"), unknown: t("未知") }[health] ?? health
  return <Badge tone={tone as BadgeTone}>{label}</Badge>
}

export function StatusBadge({ status }: { status: string }) {
  const map: Record<string, BadgeTone> = {
    completed: 'ok',
    failed: 'err',
    client_cancelled: 'warn',
    timeout: 'err',
    running: 'info',
    accepted: 'muted',
    routing: 'muted',
    active: 'ok',
    disabled: 'muted',
  }
  return <Badge tone={map[status] ?? 'muted'}>{status}</Badge>
}

export function Button({
  children,
  onClick,
  variant = 'default',
  disabled,
  type = 'button',
}: {
  children: React.ReactNode
  onClick?: () => void
  variant?: 'default' | 'primary' | 'danger' | 'ghost'
  disabled?: boolean
  type?: 'button' | 'submit'
}) {
  return (
    <button className={`btn btn-${variant}`} onClick={onClick} disabled={disabled} type={type}>
      {children}
    </button>
  )
}

export function Modal({
  title,
  onClose,
  children,
  wide,
}: {
  title: string
  onClose: () => void
  children: React.ReactNode
  wide?: boolean
}) {
  const ref = useRef<HTMLDivElement>(null)
  const titleId = useId()
  const closeRef = useRef(onClose)
  closeRef.current = onClose
  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null
    const overflow = document.body.style.overflow
    document.body.style.overflow = 'hidden'
    const focusable = () => Array.from(ref.current?.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), a[href], [tabindex="0"]') ?? [])
    ;(focusable()[0] ?? ref.current)?.focus()
    const handler = (event: KeyboardEvent) => {
      if (event.key === 'Escape') { event.stopPropagation(); closeRef.current() }
      if (event.key === 'Tab') {
        const items = focusable(), first = items[0], last = items[items.length - 1]
        if (!first) { event.preventDefault(); return }
        if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus() }
        else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus() }
      }
    }
    document.addEventListener('keydown', handler)
    return () => { document.body.style.overflow = overflow; document.removeEventListener('keydown', handler); previous?.focus() }
  }, [])
  return (
    <div className="modal-mask" onClick={onClose}>
      <div ref={ref} role="dialog" aria-modal="true" aria-labelledby={titleId} tabIndex={-1} className={`modal ${wide ? 'modal-wide' : ''}`} onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <h3 id={titleId}>{title}</h3>
          <button className="modal-close" aria-label={t('关闭')} onClick={onClose}>
            ×
          </button>
        </div>
        <div className="modal-body">{children}</div>
      </div>
    </div>
  )
}

export function Field({ label, children, hint }: { label: string; children: React.ReactNode; hint?: string }) {
  return (
    <label className="field">
      <span className="field-label">{label}</span>
      {children}
      {hint && <span className="field-hint">{hint}</span>}
    </label>
  )
}

export function EmptyState({ title, hint }: { title: string; hint?: string }) {
  return (
    <div className="empty">
      <div className="empty-title">{title}</div>
      {hint && <div className="empty-hint">{hint}</div>}
    </div>
  )
}

export function Table({ head, children }: { head: React.ReactNode[]; children: React.ReactNode }) {
  return (
    <div className="table-scroll" tabIndex={0}><table className="table">
      <thead>
        <tr>
          {head.map((h, i) => (
            <th key={i}>{h}</th>
          ))}
        </tr>
      </thead>
      <tbody>{children}</tbody>
    </table></div>
  )
}

export function Toast({ message, tone }: { message: string; tone: 'ok' | 'err' }) {
  const [dismissed, setDismissed] = useState(false)
  useEffect(() => setDismissed(false), [message])
  if (!message || dismissed) return null
  return <div role={tone === 'err' ? 'alert' : 'status'} className={`toast toast-${tone}`}><span>{message}</span>{tone === 'err' && <button className="modal-close" aria-label={t('关闭')} onClick={() => setDismissed(true)}>×</button>}</div>
}
