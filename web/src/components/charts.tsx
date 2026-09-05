import React from 'react'

/** 轻量 SVG 柱状图（requests / tokens / cost 时间序列） */
export function BarChart({
  data,
  height = 160,
  valueKey = 'requests',
  label = (v: number) => String(v),
}: {
  data: { bucket: string; [k: string]: any }[]
  height?: number
  valueKey?: string
  label?: (v: number) => string
}) {
  if (!data.length) return <div className="chart-empty">暂无数据</div>
  const values = data.map((d) => Number(d[valueKey] ?? 0))
  const max = Math.max(...values, 1)
  const width = 100 / data.length
  return (
    <div>
      <svg viewBox={`0 0 100 ${height / 3}`} preserveAspectRatio="none" style={{ width: '100%', height }}>
        {values.map((v, i) => {
          const h = (v / max) * (height / 3 - 2)
          return (
            <rect
              key={i}
              x={i * width + width * 0.15}
              y={height / 3 - h}
              width={width * 0.7}
              height={Math.max(h, v > 0 ? 0.6 : 0)}
              fill="var(--accent)"
              rx="0.4"
            />
          )
        })}
      </svg>
      <div className="chart-labels">
        {data.map((d, i) => (
          <span key={i} title={`${d.bucket}: ${label(values[i])}`}>
            {d.bucket.slice(5, 10)}
          </span>
        ))}
      </div>
    </div>
  )
}
