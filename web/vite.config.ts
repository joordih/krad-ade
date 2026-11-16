import { defineConfig, loadEnv } from "vite";
import react from "@vitejs/plugin-react";

const RUNTIME_PATHS = ["/runtime", "/pairing", "/upload", "/push", "/theme.css"];

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), "");
  const target = env.KRAD_RUNTIME ?? "http://127.0.0.1:7421";
  const proxied = Object.fromEntries(
    RUNTIME_PATHS.map((path) => [
      path,
      {
        target,
        ws: path === "/runtime",
        changeOrigin: true,
        configure: (proxy: { on: (event: string, listener: (request: { setHeader: (name: string, value: string) => void }) => void) => void }) => {
          const sameOrigin = (request: { setHeader: (name: string, value: string) => void }) =>
            request.setHeader("origin", target);
          proxy.on("proxyReq", sameOrigin);
          proxy.on("proxyReqWs", sameOrigin);
        },
      },
    ]),
  );
  return {
    plugins: [react()],
    server: { proxy: proxied },
    build: { outDir: "dist", emptyOutDir: true, sourcemap: false },
  };
});
