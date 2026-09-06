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
