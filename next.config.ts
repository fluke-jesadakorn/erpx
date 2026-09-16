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
  outputFileTracingExcludes: {
    "/*": ["./.env*", "./.runtime/**/*", "./.scratch/**/*", "./.wrangler/**/*", "./.open-next/**/*", "./fixtures/**/*", "./tests/**/*", "./Designed/**/*", "./docs/**/*"],
  },
};

export default nextConfig;
