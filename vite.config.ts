import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { fileURLToPath, URL } from 'node:url'

// Tauri 期望前端 dev 服务器固定端口，且不使用 IP 暴露
const host = process.env.TAURI_DEV_HOST

export default defineConfig({
  plugins: [vue()],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url))
    }
  },
  // Tauri 要求明确端口，避免 dev server 随机端口
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: 'ws',
          host,
          port: 1421
        }
      : undefined,
    watch: {
      // 不监听 Rust 后端变更，避免触发前端 HMR
      ignored: ['**/src-tauri/**']
    }
  },
  // Tauri webview 在生产构建中通过相对路径加载资源
  base: './'
})
