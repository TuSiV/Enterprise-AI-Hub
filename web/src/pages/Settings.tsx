import { useEffect, useState } from 'react'
import { api } from '../api/client'
import type { SystemInfo } from '../api/types'
import { formatTime } from '../api/types'
import { Card, Table } from '../components/ui'

export default function Settings() {
  const [info, setInfo] = useState<SystemInfo | null>(null)
  useEffect(() => {
    api.get<SystemInfo>('/api/v1/admin/config').then(setInfo).catch(() => {})
  }, [])

  return (
    <>
      <div className="page-head">
        <div>
          <h2>设置</h2>
          <div className="page-sub">运行模式、网关接入点与数据目录信息</div>
        </div>
      </div>

      <Card title="系统信息">
        <Table head={['项', '值']}>
          <tr>
            <td className="dim">版本</td>
            <td className="mono">{info?.version ?? '-'}</td>
          </tr>
          <tr>
            <td className="dim">运行模式</td>
            <td>{info?.mode === 'desktop' ? 'Desktop（本机）' : 'Server'}</td>
          </tr>
          <tr>
            <td className="dim">数据库</td>
            <td className="mono">{info?.dbDriver ?? '-'}</td>
          </tr>
          <tr>
            <td className="dim">启动时间</td>
            <td className="dim">{formatTime(info?.startedAt)}</td>
          </tr>
        </Table>
      </Card>

      <Card title="OpenAI 兼容接入点">
        <p className="dim" style={{ marginTop: 0 }}>
          任何 OpenAI SDK / OpenCode / LangChain 只需替换 Base URL 与 API Key：
        </p>
        <pre className="json">{`from openai import OpenAI

client = OpenAI(
    base_url="${info?.gatewayEndpoint ?? 'http://127.0.0.1:8787'}/v1",
    api_key="aih_live_<application key>",
)

resp = client.chat.completions.create(
    model="general-smart",
    messages=[{"role": "user", "content": "hello"}],
)`}</pre>
      </Card>

      <Card title="配置优先级">
        <p className="dim">
          CLI 参数 &gt; 环境变量（<code>AIHUB_MODE</code> / <code>AIHUB_DATABASE_URL</code> / <code>AIHUB_GATEWAY_PORT</code> /{' '}
          <code>AIHUB_DATA_DIR</code> 等）&gt; config.toml &gt; 默认值。
        </p>
      </Card>
    </>
  )
}
