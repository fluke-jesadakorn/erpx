import type { NextConfig } from "next";
import { realpathSync } from "node:fs";
import { dirname, isAbsolute, join, relative, sep } from "node:path";
import { developmentProductOrigin, productIds, sharedHostKeys } from "./src/server/modules/commercial/public/product-registry";
const cohortDirectory = process.env.ERPX_DATA_DIR;
// Case apps link dependencies from an immutable sibling fixture template.
// Turbopack must resolve both inside its filesystem root (see its root guide).
const isolatedBrowserRoot = process.env.NODE_ENV === "development" &&
  process.env.ERPX_ENVIRONMENT === "local" &&
  process.env.ERPX_BROWSER_SIMULATION === "thai-hotel-v1" &&
  ["http://127.0.0.1:3102", "http://127.0.0.1:3104"].includes(process.env.ERPX_PUBLIC_ORIGIN ?? "") &&
  cohortDirectory && /\/\.runtime\/browser\/[a-f0-9]{12}$/.test(cohortDirectory) &&
  process.cwd() === join(cohortDirectory, "app") ? dirname(cohortDirectory) : undefined;
const isolatedDependencyRoot = isolatedBrowserRoot && realpathSync(join(process.cwd(), "node_modules"));
const isolatedDependencyRelative = isolatedBrowserRoot && isolatedDependencyRoot
  ? relative(isolatedBrowserRoot, isolatedDependencyRoot)
  : "";
const dependencyRootIsOutside = isolatedDependencyRelative === ".." ||
  isolatedDependencyRelative.startsWith(`..${sep}`) || isAbsolute(isolatedDependencyRelative);
const isolatedTurbopackRoot = dependencyRootIsOutside && isolatedDependencyRoot
  ? dirname(isolatedDependencyRoot)
  : isolatedBrowserRoot;
const nextConfig: NextConfig = {
  // Version asset URLs so browsers and the CDN load the matching UI bundle.
  deploymentId:
    process.env.NEXT_DEPLOYMENT_ID ?? "navigation-more-20260916-v1",
  turbopack: { root: isolatedTurbopackRoot ?? process.cwd() },
  outputFileTracingRoot: isolatedTurbopackRoot ?? process.cwd(),
  output: "standalone",
  // Keep the public development console's mobile navigation unobstructed.
  // Compile/runtime errors still appear without the floating dev indicator.
  devIndicators: false,
  experimental: {
    authInterrupts: true,
    // Shared record readers add server entry points; reduce Webpack graph memory.
    webpackMemoryOptimizations: true,
    // Proxy buffers bodies before owner routes. Preserve supported 10 MiB
    // files and executed-copy multipart metadata so owners enforce their limits.
    proxyClientMaxBodySize: 11 * 1024 * 1024,
  },
  skipProxyUrlNormalize: true,
  // Retired Console bookmarks must reach the 410 boundary before Next adds
  // an automatic trailing-slash redirect. Installed environments keep policy.
  skipTrailingSlashRedirect: process.env.ERPX_CONSOLE_RETIRED === "enabled"
    && process.env.ERPX_ENVIRONMENT === "local"
    && process.env.ERPX_PRODUCT_ENVIRONMENT === "development"
    && /^https:\/\/console-dev-erpx\.jesadakorn\.com\/?$/.test(process.env.ERPX_PUBLIC_ORIGIN ?? ""),
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
    ...(process.env.ERPX_ENVIRONMENT === "local" && process.env.ERPX_PRODUCT_ROUTING === "enabled" && process.env.ERPX_PRODUCT_SSO === "enabled" && process.env.ERPX_PRODUCT_ENVIRONMENT === "development"
      ? [...productIds, ...sharedHostKeys].map(key => new URL(developmentProductOrigin(key)).hostname)
      : []),
    ...(["3102", "3104"].includes(process.env.ERPX_BROWSER_SIMULATION_PORT ?? "") ? ["dev-pms.localhost", "dev-pos.localhost"] : []),
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
    // Next compiles application modules into .next/server. The migration
    // runner reads the copied SQL tree at runtime; source files under src are
    // a false positive from tracing its dynamic readdir rooted at cwd.
    "/*": ["./.env*", "./.runtime/**/*", "./.scratch/**/*", "./.wrangler/**/*", "./.open-next/**/*", "./fixtures/**/*", "./tests/**/*", "./Designed/**/*", "./docs/**/*", "./src/**/*"],
  },
};

export default nextConfig;
