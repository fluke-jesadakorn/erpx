import { createServer, connect, type Socket } from 'node:net';
import { afterEach, expect, it, vi } from 'vitest';
import { cacheGet, cacheSet, closeRedisCache, redisCacheEnabled } from "../../../src/server/modules/runtime/adapters/runtime/redis-cache";
const redisUrl = process.env.ERPX_REDIS_URL;
afterEach(async () => { await closeRedisCache(); vi.unstubAllEnvs(); });
it('never enables persistent cache outside development', () => {
  vi.stubEnv('NODE_ENV', 'production'); vi.stubEnv('ERPX_WORK_CACHE', 'redis');
  expect(redisCacheEnabled()).toBe(false);
});
it.skipIf(!redisUrl)('bounds ERPX connection failure and reconnects after its own proxy recovers', async () => {
  const sockets = new Set<Socket>(); let offline = false;
  const proxy = createServer(socket => {
    if (offline) { socket.destroy(); return; }
    const upstream = connect(6379, '127.0.0.1'); sockets.add(socket); sockets.add(upstream);
    for (const s of [socket, upstream]) { s.on('error', () => {}); s.on('close', () => sockets.delete(s)); }
    socket.pipe(upstream); upstream.pipe(socket);
    socket.on('close', () => upstream.destroy()); upstream.on('close', () => socket.destroy());
  });
  await new Promise<void>(resolve => proxy.listen(0, '127.0.0.1', resolve));
  const address = proxy.address(); if (!address || typeof address === 'string') throw Error('PROXY_ADDRESS');
  const url = new URL(redisUrl!); url.port = String(address.port);
  vi.stubEnv('NODE_ENV', 'development'); vi.stubEnv('ERPX_WORK_CACHE', 'redis'); vi.stubEnv('ERPX_REDIS_URL', url.href);
  const key = `erpx:dev:test:${crypto.randomUUID()}`;
  try {
    await cacheSet(key, 'available', 10_000); expect(await cacheGet(key)).toBe('available');
    offline = true; for (const socket of sockets) socket.destroy();
    await new Promise(resolve => setTimeout(resolve, 30));
    const start = performance.now(); expect(await cacheGet(key)).toBeNull();
    await cacheSet(key, 'unavailable', 1000); expect(performance.now() - start).toBeLessThan(800);
    offline = false;
    let recovered: string | null = null;
    for (let i = 0; i < 30 && !recovered; i++) { await new Promise(resolve => setTimeout(resolve, 100)); recovered = await cacheGet(key); }
    expect(recovered).toBe('available');
  } finally {
    await closeRedisCache(); for (const socket of sockets) socket.destroy();
    await new Promise<void>(resolve => proxy.close(() => resolve()));
  }
});
