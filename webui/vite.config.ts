import { defineConfig } from 'vite-plus'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

// https://viteplus.dev/config/
export default defineConfig({
  plugins: [react(), tailwindcss()],
  lint: {
    ignorePatterns: ['dist/**'],
  },
  fmt: {
    singleQuote: true,
    semi: false,
  },
})
