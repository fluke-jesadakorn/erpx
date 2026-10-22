import { createClient } from "redis";

function makeClient(url: string) {
  return createClient({ url, disableOfflineQueue: true, commandsQueueMaxLength: 64,
    socket: { connectTimeout: 150, reconnectStrategy: retries => Math.min(5000, 250 * 2 ** Math.min(retries, 4)) } });
}
type Client = ReturnType<typeof makeClient>;
type Connection = { url: string; client: Client; connecting?: Promise<unknown> };
const shared = globalThis as typeof globalThis & { erpxRedisCache?: Connection };

export function redisCacheEnabled() {
  return process.env.NODE_ENV === "development" && process.env.ERPX_WORK_CACHE === "redis" && !!process.env.ERPX_REDIS_URL;
}

function connection() {
  if (!redisCacheEnabled()) return null;
  const url = process.env.ERPX_REDIS_URL!;
  if (shared.erpxRedisCache?.url !== url) {
    if (shared.erpxRedisCache?.client.isOpen) shared.erpxRedisCache.client.destroy();
    const client = makeClient(url);
    // Cache failure is not a business failure. Never log connection URLs/secrets.
    client.on("error", () => {});
    const value: Connection = { url, client };
    shared.erpxRedisCache = value;
    value.connecting = client.connect().catch(() => {}).finally(() => { value.connecting = undefined; });
  }
  return shared.erpxRedisCache!;
}

async function ready() {
  const value = connection();
  if (!value) return null;
  if (!value.client.isReady && value.connecting) {
    let timer: ReturnType<typeof setTimeout> | undefined;
    try { await Promise.race([value.connecting, new Promise(resolve => { timer = setTimeout(resolve, 150); })]); }
    finally { clearTimeout(timer); }
  }
  return value.client.isReady ? value.client.withCommandOptions({ timeout: 100 }) : null;
}

export async function cacheGet(key: string): Promise<string | null> {
  try { return await (await ready())?.get(key) ?? null; } catch { return null; }
}
export async function cacheSet(key: string, value: string, ttl: number) {
  if (ttl <= 0) return;
  try { await (await ready())?.set(key, value, { PX: Math.floor(ttl) }); } catch { /* PostgreSQL remains authoritative. */ }
}
export async function closeRedisCache() {
  const value = shared.erpxRedisCache; delete shared.erpxRedisCache;
  if (value?.client.isOpen) value.client.destroy();
}
