import type { NextConfig } from "next";
const nextConfig: NextConfig = {
  // Version asset URLs so browsers and the CDN load the matching UI bundle.
  deploymentId:
    process.env.NEXT_DEPLOYMENT_ID ?? "navigation-more-20260916-v1",
  turbopack: { root: process.cwd() },
  outputFileTracingRoot: process.cwd(),
  output: "standalone",
  skipProxyUrlNormalize: true,
  distDir: process.env.ERPX_BUILD_DIR ?? ".next",
  allowedDevOrigins: ["127.0.0.1", "erpx.jesadakorn.com"],
  poweredByHeader: false,
  headers() {
    // Development chunk names are reused as routes compile and files change.
    // Keep the public development origin's browser and CDN copies revalidated.
    return process.env.NODE_ENV === "development" ? [{
      source: "/_next/static/:path*",
      headers: [
        { key: "Cache-Control", value: "no-store, must-revalidate" },
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
