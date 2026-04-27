import path from 'node:path'
import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

// https://vite.dev/config/
export default defineConfig({
  base: '/ui/',
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
  server: {
    proxy: {
      '/info': 'http://127.0.0.1:59000',
      '/jobs': 'http://127.0.0.1:59000',
      '/stats': 'http://127.0.0.1:59000',
      '/reservations': 'http://127.0.0.1:59000',
      '/gpu-processes': 'http://127.0.0.1:59000',
    },
  },
})
