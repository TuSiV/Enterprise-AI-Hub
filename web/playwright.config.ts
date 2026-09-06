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

import { defineConfig } from '@playwright/test'

// Desktop/Web E2E（§31.6）：对 aihub-server 托管的控制台跑真实浏览器用例。
// 本地运行：npm i -D @playwright/test && npx playwright install chromium
// AIHUB_BASE_URL 指向已启动的 server（默认 http://127.0.0.1:8787）
export default defineConfig({
  testDir: './e2e',
  timeout: 30_000,
  use: {
    baseURL: process.env.AIHUB_BASE_URL ?? 'http://127.0.0.1:8787',
  },
  retries: 0,
})
