import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import path from 'path'
import { nodePolyfills } from 'vite-plugin-node-polyfills'
import wasm from 'vite-plugin-wasm'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react(), tailwindcss(), nodePolyfills(), wasm()],
  define: {
    global: 'window',
  },
  resolve: {
    alias: [
      { find: "@", replacement: path.resolve(__dirname, "./src") },
      // The Pyth SDK (@pythnetwork/solana-utils) re-exports a Jito helper that
      // imports jito-ts, which drags in an ancient @solana/web3.js@1.77.4 and an
      // incompatible rpc-websockets subpath that breaks the browser build. We
      // only use the receiver's regular transaction path, so stub jito-ts out.
      {
        find: /^jito-ts(\/.*)?$/,
        replacement: path.resolve(__dirname, "./src/stubs/jito-ts.ts"),
      },
    ],
  },
})
