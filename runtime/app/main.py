"""AI Runtime Sidecar 协议层（方案 §13.2 / §23.4 / §37）。

启动方式（由 Rust Core 管理时）：python -m app.main --port 0 --session-token <ephemeral>
端口写入 stdout（PORT=<port>），结构化日志走 stderr（stdout 不得输出敏感正文）。
"""

from __future__ import annotations

import argparse
import logging
import sys
from typing import Optional

from fastapi import FastAPI, Header, HTTPException

RUNTIME_VERSION = "0.1.0"
PROTOCOL_VERSION = "1"
CAPABILITIES = []  # parse / embedding / rerank / agent 按 M12+ 逐步启用

app = FastAPI(title="aihub-runtime", version=RUNTIME_VERSION)
_session_token: str | None = None
log = logging.getLogger("aihub.runtime")


def _auth(session_token: str | None) -> None:
    if _session_token and session_token != _session_token:
        raise HTTPException(status_code=401, detail="invalid runtime session token")


@app.get("/internal/v1/health")
def health(x_aih_session_token: Optional[str] = Header(default=None)) -> dict:
    _auth(x_aih_session_token)
    return {
        "status": "ok",
        "runtimeVersion": RUNTIME_VERSION,
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": CAPABILITIES,
    }


@app.get("/internal/v1/tasks/types")
def task_types(x_aih_session_token: Optional[str] = Header(default=None)) -> dict:
    _auth(x_aih_session_token)
    return {"data": CAPABILITIES}


def main() -> None:
    global _session_token
    parser = argparse.ArgumentParser(prog="aihub-runtime")
    parser.add_argument("--port", type=int, default=0, help="0 = 随机端口（sidecar 模式）")
    parser.add_argument("--session-token", default=None)
    args = parser.parse_args()
    _session_token = args.session_token

    logging.basicConfig(stream=sys.stderr, level=logging.INFO)

    import uvicorn

    if args.port == 0:
        import socket

        with socket.socket() as s:
            s.bind(("127.0.0.1", 0))
            port = s.getsockname()[1]
    else:
        port = args.port

    # 握手：Core 读取 stdout 的 PORT 行
    sys.stdout.write(f"PORT={port}\n")
    sys.stdout.flush()
    log.info("aihub-runtime %s (protocol %s) listening on 127.0.0.1:%s", RUNTIME_VERSION, PROTOCOL_VERSION, port)
    uvicorn.run(app, host="127.0.0.1", port=port, log_config=None)


if __name__ == "__main__":
    main()
