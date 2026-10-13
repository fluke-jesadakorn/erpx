import { existsSync } from "node:fs";
import { join, resolve } from "node:path";

export function pinnedBunBinary(root: string, environment: NodeJS.ProcessEnv = process.env) {
  const local = join(root, ".runtime/bun-1.4.3/bun-darwin-aarch64/bun");
  return environment.ERPX_BUN_BIN ?? (existsSync(local) ? local : "bun");
}

export function pinnedNodeBinary(root: string, environment: NodeJS.ProcessEnv = process.env) {
  return resolve(environment.ERPX_NODE_BIN ?? join(root, ".runtime/node/node-v26.11.1-darwin-arm64/bin/node"));
}

export function attachmentToolEnvironment(root: string, environment: NodeJS.ProcessEnv = process.env) {
  return {
    ERPX_CLAMD_SOCKET: environment.ERPX_CLAMD_SOCKET ?? join(root, ".scratch/dev/clamav/clamd.sock"),
    ERPX_CLAMAV_DATABASE_DIR: environment.ERPX_CLAMAV_DATABASE_DIR ?? join(root, ".scratch/dev/clamav/db"),
    ERPX_CLAMAV_VERSION: environment.ERPX_CLAMAV_VERSION ?? "1.5.4",
    ERPX_HEIF_CONVERT: environment.ERPX_HEIF_CONVERT ?? "/opt/homebrew/opt/libheif/bin/heif-convert",
    ERPX_HEIF_ENC: environment.ERPX_HEIF_ENC ?? "/opt/homebrew/opt/libheif/bin/heif-enc",
    ERPX_PDFTOPPM_BIN: environment.ERPX_PDFTOPPM_BIN ?? "/opt/homebrew/bin/pdftoppm",
  };
}
