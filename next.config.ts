import type { NextConfig } from "next";
const nextConfig: NextConfig = {
  // Version asset URLs so browsers and the CDN load the matching UI bundle.
  deploymentId:
    process.env.NEXT_DEPLOYMENT_ID ?? "navigation-more-20260916-v1",
  turbopack: { root: process.cwd() },
  outputFileTracingRoot: process.cwd(),
  output: "standalone",
  // Keep the public development console's mobile navigation unobstructed.
  // Compile/runtime errors still appear without the floating dev indicator.
  devIndicators: false,
  experimental: {
    authInterrupts: true,
    // Proxy buffers bodies before owner routes. Preserve supported 10 MiB
    // files and executed-copy multipart metadata so owners enforce their limits.
    proxyClientMaxBodySize: 11 * 1024 * 1024,
  },
  skipProxyUrlNormalize: true,
  distDir: process.env.ERPX_BUILD_DIR ?? ".next",
  // Isolated dev experiments let Next update their generated type globs without
  // rewriting the workspace's TypeScript configuration.
  ...(process.env.NODE_ENV === "development" && process.env.ERPX_DEV_TSCONFIG
    ? { typescript: { tsconfigPath: process.env.ERPX_DEV_TSCONFIG } }
    : {}),
  allowedDevOrigins: [
    "127.0.0.1",
    "dev-erpx.jesadakorn.com",
    "console-dev-erpx.jesadakorn.com",
    "dev-pms.jesadakorn.com",
    "dev-pos.jesadakorn.com",
    ...(process.env.ERPX_BROWSER_SIMULATION_PORT === "3104" ? ["dev-pms.localhost", "dev-pos.localhost"] : []),
  ],
  poweredByHeader: false,
  headers() {
    // Development chunk names are reused as routes compile and files change.
    // Revalidate browser copies on every request; unchanged assets can use 304.
    // Shared CDN caches must never retain development chunks.
    return process.env.NODE_ENV === "development" ? [{
      source: "/_next/static/:path*",
      headers: [
        { key: "Cache-Control", value: "private, no-cache, must-revalidate" },
        { key: "CDN-Cache-Control", value: "no-store" },
        { key: "Cloudflare-CDN-Cache-Control", value: "no-store" },
      ],
    }, {
      // Next emits content-hashed media filenames, including next/font files.
      // These are immutable even in dev; revalidation can otherwise duplicate
      // the font preload during hydration and delay every full navigation.
      source: "/_next/static/media/:path*",
      headers: [
        { key: "Cache-Control", value: "private, max-age=31536000, immutable" },
        { key: "CDN-Cache-Control", value: "no-store" },
        { key: "Cloudflare-CDN-Cache-Control", value: "no-store" },
      ],
    }] : [];
  },
  outputFileTracingExcludes: {
    "/*": ["./.env*", "./.runtime/**/*", "./.scratch/**/*", "./.wrangler/**/*", "./.open-next/**/*", "./fixtures/**/*", "./tests/**/*", "./Designed/**/*", "./docs/**/*"],
  },
};

export default nextConfig;
