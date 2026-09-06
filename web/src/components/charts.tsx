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
/** Native SVG chart with per-point titles and a readable data table. */
export function BarChart({ data, height = 220, valueKey = 'requests', label = (v: number) => String(v) }: {
  data: { bucket: string; [k: string]: any }[]; height?: number; valueKey?: string; label?: (v: number) => string
}) {
  if (!data.length) return <div className="chart-empty">{t('暂无数据')}</div>
  const values = data.map(d => Number(d[valueKey] ?? 0))
  const max = Math.max(...values, 1)
  const left = Math.min(200, Math.max(62, ...[0, .25, .5, .75, 1].map(ratio => label(max * ratio).length * 6.5 + 16)))
  const width = 760, top = 20, bottom = height - 32, plotHeight = bottom - top, step = (750 - left) / data.length
  return <div className="usage-chart">
    <svg viewBox={`0 0 ${width} ${height}`} style={{ width:'100%', height:'auto', minHeight:160 }} role="img" aria-label={t('用量趋势')}>
      {[0, .25, .5, .75, 1].map(ratio => <g key={ratio}><line x1={left} x2="750" y1={bottom-ratio*plotHeight} y2={bottom-ratio*plotHeight} stroke="var(--border)" strokeDasharray="3 5" /><text x={left - 8} y={bottom-ratio*plotHeight+4} textAnchor="end" fill="var(--text-dim)" fontSize="10">{label(max*ratio)}</text></g>)}
      {values.map((value,i) => { const h = value/max*plotHeight; return <g key={data[i].bucket}><rect x={left+i*step+step*.2} y={bottom-h} width={Math.max(step*.6,1)} height={h} fill="var(--accent)" opacity=".85" rx="3"><title>{`${data[i].bucket}: ${label(value)}`}</title></rect>{(i % Math.max(1,Math.ceil(data.length/7)) === 0 || i === data.length-1) && <text x={left+i*step+step*.5} y={height-8} textAnchor="middle" fill="var(--text-dim)" fontSize="10">{data[i].bucket.slice(5,10)}</text>}</g> })}
    </svg>
    <details className="chart-data"><summary>{t('查看明细数据')}</summary><div className="table-scroll"><table className="table"><thead><tr><th>{t('时间')}</th><th>{t('值')}</th></tr></thead><tbody>{data.map((point,i) => <tr key={point.bucket}><td>{point.bucket}</td><td>{label(values[i])}</td></tr>)}</tbody></table></div></details>
  </div>
}
