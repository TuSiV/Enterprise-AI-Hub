import { t } from '../i18n'
import { useEffect, useState } from 'react'
import { api } from '../api/client'
import { Badge, Button, Card, EmptyState, Field, Table, Toast } from '../components/ui'

export default function Security() {
  const [routing, setRouting] = useState<any[]>([])
  const [policies, setPolicies] = useState<any[]>([])
  const [users, setUsers] = useState<any[]>([])
  const [runtime, setRuntime] = useState<any>(null)
  const [dlpInput, setDlpInput] = useState(t("联系人 13812345678，key 是 sk-abc123456789"))
  const [dlpResult, setDlpResult] = useState<any>(null)
  const [toast, setToast] = useState<{ message: string; tone: 'ok' | 'err' }>({ message: '', tone: 'ok' })

  const notify = (message: string, tone: 'ok' | 'err' = 'ok') => {
    setToast({ message, tone })
    if (tone !== 'err') setTimeout(() => setToast({ message: '', tone }), 2600)
  }

  const load = () =>
    Promise.all([
      api.get<any[]>('/api/v1/admin/security/routing-policies'),
      api.get<any[]>('/api/v1/admin/security/policies'),
      api.get<any[]>('/api/v1/admin/users'),
      api.get<any>('/api/v1/admin/runtime/status'),
    ]).then(([r, p, u, rt]) => {
      setRouting(r)
      setPolicies(p)
      setUsers(u)
      setRuntime(rt)
    })
  useEffect(() => {
    load().catch((e) => notify(e.message, 'err'))
  }, [])

  const scan = async () => {
    try {
      setDlpResult(await api.post('/api/v1/admin/security/dlp/scan', { content: dlpInput }))
    } catch (e: any) {
      notify(e.message, 'err')
    }
  }

  return (
    <>
      <div className="page-head">
        <div>
          <h2>{t("安全治理")}</h2>
          <div className="page-sub">{t("数据分级→模型策略 · DLP 脱敏 · SSRF 校验 · RBAC 用户")}</div>
        </div>
      </div>

      <div className="grid-2">
        <Card title={t("DLP 扫描测试")}>
          <Field label={t("待扫描内容")}>
            <textarea rows={3} value={dlpInput} onChange={(e) => setDlpInput(e.target.value)} />
          </Field>
          <Button variant="primary" onClick={scan}>
            {t("扫描")}</Button>
          {dlpResult && (
            <div style={{ marginTop: 12 }}>
              <p>
                {t("动作：")}<Badge tone={dlpResult.action === 'block' ? 'err' : dlpResult.action === 'mask' ? 'warn' : 'ok'}>
                  {dlpResult.action}
                </Badge>
              </p>
              <p className="mono dim" style={{ wordBreak: 'break-all' }}>
                {dlpResult.content}
              </p>
              {dlpResult.hits?.length > 0 && (
                <Table head={[t("规则"), t("动作"), t("命中")]}>
                  {dlpResult.hits.map((h: any, i: number) => (
                    <tr key={i}>
                      <td className="mono">{h.rule}</td>
                      <td>{h.action}</td>
                      <td>{h.count}</td>
                    </tr>
                  ))}
                </Table>
              )}
            </div>
          )}
        </Card>

        <Card title={t("Runtime 状态（M11）")}>
          {!runtime ? (
            <EmptyState title={t("加载中…")} />
          ) : (
            <>
              <p>
                {t("状态：")}<Badge tone={runtime.status === 'running' ? 'ok' : runtime.status === 'failed' ? 'err' : 'muted'}>
                  {runtime.status}
                </Badge>
              </p>
              {runtime.endpoint && <p className="mono dim">{runtime.endpoint}</p>}
              {runtime.error && <p style={{ color: 'var(--err)' }}>{runtime.error}</p>}
              <p className="dim" style={{ fontSize: 12 }}>
                {t("PDF/DOCX 解析依赖 Python Runtime；managed 模式由应用自动拉起并崩溃重启。")}</p>
            </>
          )}
        </Card>
      </div>

      <Card title={t("路由策略（routing_policies）")}>
        {routing.length ? (
          <Table head={['Key', t("优先级"), t("匹配"), t("动作"), t("启用")]}>
            {routing.map((p) => (
              <tr key={p.id}>
                <td className="mono">{p.key}</td>
                <td>{p.priority}</td>
                <td className="mono dim">{JSON.stringify(p.matchRules)}</td>
                <td className="mono dim">{JSON.stringify(p.action)}</td>
                <td>{p.enabled ? <Badge tone="ok">{t("是")}</Badge> : <Badge tone="muted">{t("否")}</Badge>}</td>
              </tr>
            ))}
          </Table>
        ) : (
          <EmptyState title={t("暂无路由策略")} hint={t("策略可在数据库/后续管理界面配置：按应用/任务/数据分级路由到指定 Virtual Model")} />
        )}
      </Card>

      <Card title={t("安全策略（DLP/分级/SSRF）")}>
        {policies.length ? (
          <Table head={['Key', t("类型"), t("规则"), t("启用")]}>
            {policies.map((p) => (
              <tr key={p.id}>
                <td className="mono">{p.key}</td>
                <td>
                  <Badge tone="info">{p.policyType}</Badge>
                </td>
                <td className="mono dim" style={{ maxWidth: 380, overflow: 'hidden', textOverflow: 'ellipsis' }}>
                  {JSON.stringify(p.rule)}
                </td>
                <td>{p.enabled ? <Badge tone="ok">{t("是")}</Badge> : <Badge tone="muted">{t("否")}</Badge>}</td>
              </tr>
            ))}
          </Table>
        ) : (
          <EmptyState title={t("暂无自定义策略")} hint={t("内置规则：手机号脱敏、密钥 token 脱敏、SSRF 目标校验、数据分级矩阵")} />
        )}
      </Card>

      <Card title={t("用户与角色（RBAC）")}>
        {users.length ? (
          <Table head={[t("用户"), t("显示名"), t("身份源"), t("状态")]}>
            {users.map((u) => (
              <tr key={u.id}>
                <td className="mono">{u.username ?? u.id.slice(0, 8)}</td>
                <td>{u.displayName}</td>
                <td>{u.identityProvider}</td>
                <td>
                  <Badge tone={u.status === 'active' ? 'ok' : 'muted'}>{u.status}</Badge>
                </td>
              </tr>
            ))}
          </Table>
        ) : (
          <EmptyState title={t("暂无用户")} hint={t("POST /api/v1/admin/users 创建；登录走 /api/v1/auth/login")} />
        )}
      </Card>
      <Toast message={toast.message} tone={toast.tone} />
    </>
  )
}
