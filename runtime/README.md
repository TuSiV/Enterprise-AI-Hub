# aihub-runtime：Python AI Runtime Sidecar（方案 M11+）

Rust Core 的 AI 执行引擎，职责仅限 AI 任务（Parse / Embedding / Rerank / RAG / Agent / Eval），
不拥有 Provider、Application、Quota、Cost 等控制面数据（方案 §3.2）。

## 协议

- 传输：HTTP（loopback / 内网），请求必须携带 Runtime Session Token。
- 握手：Core 以 `aihub-runtime --port 0 --session-token <ephemeral>` 启动，
  Runtime 将实际端口打印到 stdout（`PORT=<port>`），随后 Core 做健康检查（§23.4）。
- 版本协商：`/internal/v1/health` 返回 `runtimeVersion` / `protocolVersion` / `capabilities`；
  Core 发现协议不兼容时拒绝启用高级功能（方案 §37）。

## 启动

```bash
pip install -r requirements.txt
python -m app.main --port 0 --session-token <token>
```

## 已实现端点（协议层）

| 端点 | 说明 |
|---|---|
| `GET /internal/v1/health` | 版本握手 + 能力列表 |
| `GET /internal/v1/tasks/types` | 已注册任务类型 |

任务执行端点（parse / embeddings / rerank / retrieval / agents/run / evals/run）在 M12+ 按方案 §13.2 逐个接入。
