import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// 开发时代理到本地 AI Hub（网关 8787 与 Admin API 同端口）
export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: {
      '/api': 'http://127.0.0.1:8787',
      '/v1': 'http://127.0.0.1:8787',
      '/health': 'http://127.0.0.1:8787',
    },
  },
})
