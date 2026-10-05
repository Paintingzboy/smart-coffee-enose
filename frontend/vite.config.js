import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// Saat `npm run dev`, request /api diteruskan ke backend Rust lokal (port 3000).
export default defineConfig({
  plugins: [react()],
  server: {
    host: true,
    proxy: { '/api': 'http://localhost:3000' },
  },
  build: { chunkSizeWarningLimit: 1200 },
})
