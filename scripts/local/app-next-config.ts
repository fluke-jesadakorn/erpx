import type { NextConfig } from "next";
import { resolve } from "node:path";
import { appDefinition } from "@erpx/contracts/apps";
import base from "../../next.config.ts";
import {holdingRoutingEnabled,holdingHosts} from '@erpx/contracts/holding-hosts';

export const workspaceRoot = resolve(import.meta.dirname, "../..");
export function appNextConfig(id: string): NextConfig {
  if (id !== "compat") appDefinition(id);
  return { ...base, distDir: process.env.ERPX_BUILD_DIR ?? ".next",
    ...(holdingRoutingEnabled(process.env)&&id!=='account'&&id!=='compat'?{
      assetPrefix:`/__erpx/${id}`,
      images:{...base.images,path:`/__erpx/${id}/_next/image`},
      allowedDevOrigins:[...holdingHosts.map(host=>host.hostname),'127.0.0.1'],
    }:{}),
    outputFileTracingRoot: base.outputFileTracingRoot ?? workspaceRoot, turbopack: { ...base.turbopack, root: base.turbopack?.root ?? workspaceRoot },
    deploymentId: process.env.NEXT_DEPLOYMENT_ID ?? `development-${id}`,
    // Each application owns its generated route types and compiler cache.
    typescript: { tsconfigPath: "tsconfig.json" },
  };
}
