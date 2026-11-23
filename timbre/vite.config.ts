import { readFileSync } from 'node:fs'
import { fileURLToPath, URL } from 'node:url'

import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { templateCompilerOptions } from '@tresjs/core'
import vueDevTools from 'vite-plugin-vue-devtools'

const pkg = JSON.parse(readFileSync(new URL('./package.json', import.meta.url), 'utf8')) as {
  version: string
}

// https://vite.dev/config/
export default defineConfig({
  // templateCompilerOptions lets Vue treat <Tres*> tags as TresJS elements.
  plugins: [vue({ ...templateCompilerOptions }), vueDevTools()],
  define: {
    __TIMBRE_VERSION__: JSON.stringify(pkg.version),
  },
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  server: {
    // The browser only talks to rhythm; in development Vite proxies /api to it.
    proxy: {
      '/api': process.env.QUARTET_RHYTHM_URL ?? 'http://127.0.0.1:8080',
    },
  },
})
