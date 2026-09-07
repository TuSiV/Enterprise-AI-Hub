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

# Enterprise AI Hub 端到端冒烟 —— Windows 版（与 scripts/smoke.sh 等价）：
# 启动 mock 上游 + aihub-server，走 Admin API 配置 Provider/Model/VirtualModel/Application，
# 然后以 OpenAI 兼容方式调用 Gateway（非流式 + 流式），校验 Usage/Cost/Audit 落库。
#
# 前置条件：
#   - 已执行 cargo build（target\debug\aihub-server.exe / aihub-mock-openai.exe 存在）
#   - Python 3 在 PATH（命令名 python）
#   - PowerShell 5.1+（Windows 10/11 自带）

$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8

$Root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$DataDir = Join-Path ([IO.Path]::GetTempPath()) ("aihub-smoke-" + [Guid]::NewGuid().ToString('N'))
$MockPort = 9901
$ServerPort = 18790
$MockExe = Join-Path $Root "target\debug\aihub-mock-openai.exe"
$ServerExe = Join-Path $Root "target\debug\aihub-server.exe"

if (-not (Test-Path $MockExe)) { throw "mock 未编译：$MockExe（先 cargo build）" }
if (-not (Test-Path $ServerExe)) { throw "server 未编译：$ServerExe（先 cargo build）" }

function Stop-Tree($Proc) {
  if ($Proc -and -not $Proc.HasExited) {
    taskkill /PID $Proc.Id /T /F 2>$null | Out-Null
  }
}

# PowerShell 5.1 无 try/finally 之外的进程清理 trap，用 finally + 手动清理
$ServerProc = $null
$MockProc = $null
try {
  New-Item -ItemType Directory -Force -Path $DataDir | Out-Null
  Write-Host "▶ data dir: $DataDir"

  # 1. 启动 mock 上游
  $MockProc = Start-Process -FilePath $MockExe -PassThru -NoNewWindow
  Start-Sleep -Milliseconds 500

  # 2. 启动 AI Hub（desktop 模式语义：本地 SQLite + loopback）
  $env:AIHUB_DATA_DIR = $DataDir
  $env:AIHUB_SECRET_BACKEND = 'memory'
  $ServerProc = Start-Process -FilePath $ServerExe -ArgumentList "--port", "$ServerPort", "--mode", "desktop" `
    -PassThru -NoNewWindow

  $live = $false
  for ($i = 0; $i -lt 40; $i++) {
    Start-Sleep -Milliseconds 250
    try {
      Invoke-RestMethod -Uri "http://127.0.0.1:$ServerPort/health/live" -TimeoutSec 2 | Out-Null
      $live = $true
      break
    } catch { }
  }
  if (-not $live) { throw "server 未在预期时间内 live" }
  Write-Host "✓ server live"

  $TokenFile = Join-Path $DataDir "admin_token"
  $Token = (Get-Content $TokenFile -Raw).Trim()
  $Auth = "Authorization: Bearer $Token"
  $Base = "http://127.0.0.1:$ServerPort"

  function Invoke-Json($Method, $Path, $Body) {
    $params = @{
      Method  = $Method
      Uri     = "$Base$Path"
      Headers = @{ Authorization = $Auth }
      ContentType = 'application/json'
      TimeoutSec = 15
    }
    if ($Body) { $params.Body = $Body }
    Invoke-RestMethod @params
  }

  # 3. Admin API：创建 Provider（凭据入 SecretStore）→ 测试连接 → 发现模型
  $Provider = Invoke-Json Post '/api/v1/admin/providers' (@{
    key = 'mock-primary'; name = 'Mock Primary'; kind = 'openai_compatible'
    baseUrl = "http://127.0.0.1:$MockPort/v1"; apiKey = 'sk-mock-secret'
    timeoutMs = 15000; enabled = $true
  } | ConvertTo-Json -Depth 5)
  $ProviderId = $Provider.data.id
  Write-Host "✓ provider created: $ProviderId"

  $Test = Invoke-Json Post "/api/v1/admin/providers/$ProviderId/test" '{}'
  if (-not $Test.ok) { throw "connection test failed" }
  Write-Host "✓ connection test ok"

  Invoke-Json Post "/api/v1/admin/providers/$ProviderId/discover-models" '{}' | Out-Null
  Write-Host "✓ models discovered"

  $Models = Invoke-Json Get '/api/v1/admin/models'
  $Model = $Models.data | Where-Object { $_.modelKey -eq 'mock-mini' } | Select-Object -First 1
  if (-not $Model) { throw "mock-mini 未发现" }
  $ModelId = $Model.id

  # 4. 配置模型定价 + 绑定预置 Virtual Model: general-smart → mock-mini
  Invoke-Json Patch "/api/v1/admin/models/$ModelId" (@{
    pricing = @{ currency = 'USD'; unitTokens = 1000000; input = 1.0; output = 2.0 }
  } | ConvertTo-Json -Depth 5) | Out-Null
  Invoke-Json Put '/api/v1/admin/virtual-models/general-smart/targets' (@{
    targets = @(@{ modelId = $ModelId; priority = 10 })
  } | ConvertTo-Json -Depth 5) | Out-Null
  Write-Host "✓ general-smart → mock-mini (pricing configured)"

  # 5. Application + API Key
  $App = Invoke-Json Post '/api/v1/admin/applications' (@{ key = 'smoke-app'; name = 'Smoke App' } | ConvertTo-Json)
  $AppId = $App.data.id
  $Key = Invoke-Json Post "/api/v1/admin/applications/$AppId/keys" (@{ name = 'smoke' } | ConvertTo-Json)
  $ApiKey = $Key.data.plaintext
  Write-Host "✓ application key created"

  # 6. OpenAI 兼容调用（非流式）
  $Resp = Invoke-RestMethod -Method Post -Uri "$Base/v1/chat/completions" `
    -Headers @{ Authorization = "Bearer $ApiKey" } -ContentType 'application/json' `
    -Body '{"model":"general-smart","messages":[{"role":"user","content":"你好，AI Hub"}]}'
  $Content = $Resp.choices[0].message.content
  if (-not $Content.StartsWith('mock-echo')) { throw "unexpected content: $Content" }
  if ($Resp.usage.total_tokens -le 0) { throw "usage missing" }
  Write-Host "✓ non-stream chat: $Content"

  # 7. 流式（SSE 手工解析，Invoke-RestMethod 会缓冲流，改用 HttpWebRequest 逐行读）
  $req = [Net.HttpWebRequest]::Create("$Base/v1/chat/completions")
  $req.Method = 'POST'
  $req.ContentType = 'application/json'
  $req.Headers.Add('Authorization', "Bearer $ApiKey")
  $bodyBytes = [Text.Encoding]::UTF8.GetBytes('{"model":"general-smart","messages":[{"role":"user","content":"流式测试"}],"stream":true}')
  $reqStream = $req.GetRequestStream()
  $reqStream.Write($bodyBytes, 0, $bodyBytes.Length)
  $reqStream.Close()
  $reader = New-Object IO.StreamReader($req.GetResponse().GetResponseStream(), [Text.Encoding]::UTF8)
  $Content = ''
  $done = $false
  while (-not $reader.EndOfStream) {
    $line = $reader.ReadLine().Trim()
    if (-not $line.StartsWith('data:')) { continue }
    $payload = $line.Substring(5).Trim()
    if ($payload -eq '[DONE]') { $done = $true; break }
    $chunk = $payload | ConvertFrom-Json
    foreach ($choice in $chunk.choices) {
      if ($choice.delta.content) { $Content += $choice.delta.content }
    }
  }
  $reader.Close()
  if (-not $done) { throw "missing [DONE]" }
  if ($Content -notmatch '流式测试') { throw "stream content missing: $Content" }
  Write-Host "✓ stream chat: $Content"

  # 8. Usage / Cost / Audit 落库校验
  $Summary = Invoke-Json Get '/api/v1/admin/usage/summary'
  $s = $Summary.data
  if ($s.requests -lt 2 -or $s.totalTokens -le 0 -or $s.costMicrounits -le 0) { throw "usage summary 异常: $($s | ConvertTo-Json -Compress)" }
  Write-Host "✓ usage: requests=$($s.requests) tokens=$($s.totalTokens) cost=$($s.costMicrounits)microunits"

  $Requests = Invoke-Json Get '/api/v1/admin/requests?pageSize=5'
  $items = $Requests.data
  if (-not $items) { throw "no requests" }
  if ($items[0].resolvedModelKey -ne 'mock-mini' -or $items[0].status -ne 'completed') {
    throw "request explorer 异常: $($items[0] | ConvertTo-Json -Compress)"
  }
  Write-Host "✓ request explorer: resolved to mock-mini, status completed"

  $Audit = Invoke-Json Get '/api/v1/admin/audit?pageSize=5'
  if ($Audit.meta.total -le 0) { throw "no audit events" }
  Write-Host "✓ audit events: $($Audit.meta.total)"

  Write-Host ""
  Write-Host "🎉 Smoke passed — OpenAI SDK 兼容调用、路由、Usage/Cost/Audit 全链路 OK"
}
finally {
  Stop-Tree $ServerProc
  Stop-Tree $MockProc
  if (Test-Path $DataDir) { Remove-Item -Recurse -Force $DataDir -ErrorAction SilentlyContinue }
}
