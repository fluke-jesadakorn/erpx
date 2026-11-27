import { readFileSync } from "node:fs";

export type NativeApiPathsConfig = {
  holdingPrefixes?: string[];
  holdingPaths?: { method: string; path: string }[];
  platform?: { host: string; method: string; path: string }[];
  rewrites?: string[];
};

type Host = { hostname: string; holdingSlug: string };

/** Expand the checked-in cutover config into the gateway's env format. */
export function expandNativeApiPaths(config: NativeApiPathsConfig, hosts: readonly Host[]) {
  const entries: string[] = [];
  for (const prefix of config.holdingPrefixes ?? []) {
    const cleaned = prefix.replace(/^\/+/, "").replace(/\/+$/, "");
    if (!cleaned || cleaned.includes("|") || cleaned.includes(",") || cleaned.split("/").some(part => !part || part === "." || part === ".."))
      throw new Error(`INVALID_NATIVE_HOLDING_PREFIX:${prefix}`);
    for (const host of hosts) entries.push(`${host.hostname}|/api/v1/holdings/${host.holdingSlug}/${cleaned}`);
  }
  for (const entry of config.holdingPaths ?? []) {
    if (!/^[A-Z]+$/.test(entry.method)) throw new Error("INVALID_NATIVE_HOLDING_METHOD");
    const cleaned = entry.path.replace(/^\/+/, "").replace(/\/+$/, "");
    if (!cleaned || cleaned.includes("|") || cleaned.includes(",") || cleaned.split("/").some(part => !part || part === "." || part === ".."))
      throw new Error(`INVALID_NATIVE_HOLDING_PATH:${entry.path}`);
    for (const host of hosts) entries.push(`${host.hostname}|${entry.method}|/api/v1/holdings/${host.holdingSlug}/${cleaned}`);
  }
  for (const entry of config.platform ?? []) {
    if (!entry.host || !/^[A-Z]+$/.test(entry.method) || !entry.path.startsWith("/api/")) throw new Error("INVALID_NATIVE_PLATFORM_ENTRY");
    entries.push(`${entry.host}|${entry.method}|${entry.path}`);
  }
  return {
    paths: entries.join(","),
    rewrites: (config.rewrites ?? []).join(",") || undefined,
  };
}

export function loadNativeApiPaths(path: string): NativeApiPathsConfig {
  return JSON.parse(readFileSync(path, "utf8")) as NativeApiPathsConfig;
}
