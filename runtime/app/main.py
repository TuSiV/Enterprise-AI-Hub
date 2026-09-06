# Copyright 2026 YONGZHE CHEN
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

"""AI Runtime Sidecar 协议层（方案 §13.2 / §23.4 / §37）。

启动方式（由 Rust Core 管理时）：python -m app.main --port 0 --session-token <ephemeral>
端口写入 stdout（PORT=<port>），结构化日志走 stderr（stdout 不得输出敏感正文）。
"""

from __future__ import annotations

import argparse
import logging
import sys
from typing import Optional

import base64

from fastapi import FastAPI, Header, HTTPException

RUNTIME_VERSION = "0.1.0"
PROTOCOL_VERSION = "1"
CAPABILITIES = ["parse.txt", "parse.md", "parse.csv", "parse.json"]


def _optional_parse_capabilities() -> None:
    try:
        import pypdf  # noqa: F401

        CAPABILITIES.append("parse.pdf")
    except Exception:
        pass
    try:
        import docx  # type: ignore  # noqa: F401

        CAPABILITIES.append("parse.docx")
    except Exception:
        pass


_optional_parse_capabilities()

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


@app.post("/internal/v1/parse")
def parse(body: dict, x_aih_session_token: Optional[str] = Header(default=None)) -> dict:
    _auth(x_aih_session_token)
    filename = body.get("filename", "document.txt")
    mime = body.get("mimeType", "text/plain")
    data = base64.b64decode(body.get("contentBase64", ""))
    lower = (filename + " " + mime).lower()
    if "pdf" in lower:
        text, pages = _parse_pdf(data)
    elif "docx" in lower:
        text, pages = _parse_docx(data)
    else:
        text = data.decode("utf-8", errors="replace")
        pages = [{"page": 1, "text": text}]
    return {"text": text, "pages": pages}


@app.post("/internal/v1/embeddings")
def embeddings_task(body: dict, x_aih_session_token: Optional[str] = Header(default=None)) -> dict:
    """Embedding 由 Core 侧 Provider Adapter 执行（§41.3：Runtime 不持有主库凭据）；
    本地模型 Runtime 接入后在此返回 200 并更新 capabilities。"""
    raise HTTPException(status_code=501, detail="embeddings are executed by the core provider adapters")


@app.post("/internal/v1/rerank")
def rerank_task(body: dict, x_aih_session_token: Optional[str] = Header(default=None)) -> dict:
    raise HTTPException(status_code=501, detail="rerank adapter endpoint not bound (KB.rerank_model_id reserved)")


@app.post("/internal/v1/retrieval/query")
def retrieval_task(body: dict, x_aih_session_token: Optional[str] = Header(default=None)) -> dict:
    raise HTTPException(status_code=501, detail="retrieval executes in core against the vector store")


@app.post("/internal/v1/agents/run")
def agents_run(body: dict, x_aih_session_token: Optional[str] = Header(default=None)) -> dict:
    raise HTTPException(status_code=501, detail="agent loop executes in core")


@app.post("/internal/v1/evals/run")
def evals_run(body: dict, x_aih_session_token: Optional[str] = Header(default=None)) -> dict:
    raise HTTPException(status_code=501, detail="eval executes in core")


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


def _parse_pdf(data: bytes) -> tuple[str, list]:
    try:
        import io

        import pypdf
    except Exception as exc:  # pragma: no cover
        raise HTTPException(status_code=415, detail="pdf support requires the 'pypdf' package") from exc
    reader = pypdf.PdfReader(io.BytesIO(data))
    pages = []
    for i, page in enumerate(reader.pages):
        text = page.extract_text() or ""
        pages.append({"page": i + 1, "text": text})
    return "\n\n".join(p["text"] for p in pages), pages


def _parse_docx(data: bytes) -> tuple[str, list]:
    try:
        import io

        import docx
    except Exception as exc:  # pragma: no cover
        raise HTTPException(status_code=415, detail="docx support requires the 'python-docx' package") from exc
    document = docx.Document(io.BytesIO(data))
    paragraphs = [p.text for p in document.paragraphs if p.text.strip()]
    return "\n\n".join(paragraphs), [{"page": 1, "text": "\n".join(paragraphs)}]
