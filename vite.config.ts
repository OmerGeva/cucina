import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ['**/src-tauri/**', '**/target/**'] },
  },
  build: {
    target: 'safari15',
    // The app is loaded from disk; readable output is worth more than bytes.
    minify: 'esbuild',
    sourcemap: false,
  },
})
