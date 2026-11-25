import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
  build: {
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (!id.includes('node_modules')) return undefined
          if (id.includes('pdfjs-dist')) return 'vendor-pdfjs'
          if (id.includes('heic-to')) return 'vendor-heic'
          if (id.includes('dompurify')) return 'vendor-dompurify'
          if (id.includes('lucide-react')) return 'vendor-icons'
          if (id.includes('react-dom') || id.includes('/react/')) return 'vendor-react'
          return undefined
        },
      },
    },
  },
  server: {
    host: '127.0.0.1',
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ['**/src-tauri/target/**'],
    },
  },
})
