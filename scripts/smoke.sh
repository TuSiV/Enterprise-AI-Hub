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

#!/usr/bin/env bash
# Enterprise AI Hub 端到端冒烟（方案附录 G Sprint 完成判定）：
# 启动 mock 上游 + aihub-server，走 Admin API 配置 Provider/Model/VirtualModel/Application，
# 然后以 OpenAI 兼容方式调用 Gateway（非流式 + 流式），校验 Usage/Cost/Audit 落库。
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DATA_DIR="${TMPDIR:-/tmp}/aihub-smoke-$$"
MOCK_PORT=9901
SERVER_PORT=18790
ADMIN_TOKEN_FILE="$DATA_DIR/admin_token"

cleanup() {
  [[ -n "${SERVER_PID:-}" ]] && kill "$SERVER_PID" 2>/dev/null || true
  [[ -n "${MOCK_PID:-}" ]] && kill "$MOCK_PID" 2>/dev/null || true
  rm -rf "$DATA_DIR"
}
trap cleanup EXIT

mkdir -p "$DATA_DIR"
echo "▶ data dir: $DATA_DIR"

# 1. 启动 mock 上游
"$ROOT/target/debug/aihub-mock-openai" &
MOCK_PID=$!
sleep 0.5

# 2. 启动 AI Hub（desktop 模式语义：本地 SQLite + loopback）
AIHUB_DATA_DIR="$DATA_DIR" AIHUB_SECRET_BACKEND=memory "$ROOT/target/debug/aihub-server" --port "$SERVER_PORT" --mode desktop &
SERVER_PID=$!

for i in $(seq 1 40); do
  curl -sf "http://127.0.0.1:$SERVER_PORT/health/live" >/dev/null && break
  sleep 0.25
done
echo "✓ server live"

TOKEN=$(cat "$ADMIN_TOKEN_FILE")
AUTH="Authorization: Bearer $TOKEN"
BASE="http://127.0.0.1:$SERVER_PORT"
JSON() { curl -sf -X "$1" "$BASE$2" -H "$AUTH" -H 'Content-Type: application/json' ${3:+-d "$3"}; }

# 3. Admin API：创建 Provider（凭据入 SecretStore）→ 测试连接 → 发现模型
PROVIDER=$(JSON POST /api/v1/admin/providers '{
  "key": "mock-primary", "name": "Mock Primary", "kind": "openai_compatible",
  "baseUrl": "http://127.0.0.1:'"$MOCK_PORT"'/v1", "apiKey": "sk-mock-secret",
  "timeoutMs": 15000, "enabled": true }')
PROVIDER_ID=$(echo "$PROVIDER" | python3 -c 'import sys,json; print(json.load(sys.stdin)["data"]["id"])')
echo "✓ provider created: $PROVIDER_ID"

JSON POST "/api/v1/admin/providers/$PROVIDER_ID/test" '{}' | grep -q '"ok":true' && echo "✓ connection test ok"

JSON POST "/api/v1/admin/providers/$PROVIDER_ID/discover-models" '{}' >/dev/null
echo "✓ models discovered"

MODEL=$(JSON GET /api/v1/admin/models | python3 -c '
import sys, json
models = [m for m in json.load(sys.stdin)["data"] if m["modelKey"] == "mock-mini"]
print(json.dumps(models[0]))')
MODEL_ID=$(echo "$MODEL" | python3 -c 'import sys,json; print(json.load(sys.stdin)["id"])')

# 4. 配置模型定价 + 绑定预置 Virtual Model: general-smart → mock-mini
JSON PATCH "/api/v1/admin/models/$MODEL_ID" '{
  "pricing": {"currency": "USD", "unitTokens": 1000000, "input": 1.0, "output": 2.0} }' >/dev/null
JSON PUT /api/v1/admin/virtual-models/general-smart/targets "
  {\"targets\": [{\"modelId\": \"$MODEL_ID\", \"priority\": 10}]}" >/dev/null
echo "✓ general-smart → mock-mini (pricing configured)"

# 5. Application + API Key
APP=$(JSON POST /api/v1/admin/applications '{"key": "smoke-app", "name": "Smoke App"}')
APP_ID=$(echo "$APP" | python3 -c 'import sys,json; print(json.load(sys.stdin)["data"]["id"])')
KEY=$(JSON POST "/api/v1/admin/applications/$APP_ID/keys" '{"name": "smoke"}')
API_KEY=$(echo "$KEY" | python3 -c 'import sys,json; print(json.load(sys.stdin)["data"]["plaintext"])')
echo "✓ application key created"

# 6. OpenAI 兼容调用（非流式）
RESP=$(curl -sf "$BASE/v1/chat/completions" \
  -H "Authorization: Bearer $API_KEY" -H 'Content-Type: application/json' \
  -d '{"model":"general-smart","messages":[{"role":"user","content":"你好，AI Hub"}]}')
echo "$RESP" | python3 -c '
import sys, json
r = json.load(sys.stdin)
content = r["choices"][0]["message"]["content"]
assert content.startswith("mock-echo"), content
assert r["usage"]["total_tokens"] > 0
print("✓ non-stream chat:", content)'

# 7. 流式
curl -sfN "$BASE/v1/chat/completions" \
  -H "Authorization: Bearer $API_KEY" -H 'Content-Type: application/json' \
  -d '{"model":"general-smart","messages":[{"role":"user","content":"流式测试"}],"stream":true}' \
  | python3 -c '
import sys, json
content = ""
done = False
for line in sys.stdin:
    line = line.strip()
    if not line.startswith("data:"):
        continue
    payload = line[5:].strip()
    if payload == "[DONE]":
        done = True
        break
    chunk = json.loads(payload)
    for choice in chunk.get("choices", []):
        content += choice.get("delta", {}).get("content", "")
assert done, "missing [DONE]"
assert "流式测试" in content, content
print("✓ stream chat:", content)'

# 8. Usage / Cost / Audit 落库校验
SUMMARY=$(JSON GET /api/v1/admin/usage/summary)
echo "$SUMMARY" | python3 -c '
import sys, json
s = json.load(sys.stdin)["data"]
assert s["requests"] >= 2, s
assert s["totalTokens"] > 0, s
assert s["costMicrounits"] > 0, s
print("✓ usage: requests=%s tokens=%s cost=%smicrounits" % (s["requests"], s["totalTokens"], s["costMicrounits"]))'

REQUESTS=$(JSON GET '/api/v1/admin/requests?pageSize=5')
echo "$REQUESTS" | python3 -c '
import sys, json
items = json.load(sys.stdin)["data"]
assert items, "no requests"
assert items[0]["resolvedModelKey"] == "mock-mini"
assert items[0]["status"] == "completed"
print("✓ request explorer: resolved to mock-mini, status completed")'

AUDIT=$(JSON GET '/api/v1/admin/audit?pageSize=5')
echo "$AUDIT" | python3 -c '
import sys, json
total = json.load(sys.stdin)["meta"]["total"]
assert total > 0
print("✓ audit events: %s" % total)'

echo ""
echo "🎉 Smoke passed — OpenAI SDK 兼容调用、路由、Usage/Cost/Audit 全链路 OK"
