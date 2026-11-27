import { createServer } from 'node:http';
import { expect, it } from 'vitest';
import { proxyNativeApi } from '../../../scripts/local/native-api-proxy';

const endpoint = '/api/v1/holdings/acme/records/support/rec-1';
const actorContext = {
  userId: 'current-user',
  companyId: 'current-company',
  assignmentId: 'current-assignment',
  revision: 7,
  assignmentVersion: 3,
};

async function listen(server: ReturnType<typeof createServer>) {
  await new Promise<void>((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  const address = server.address();
  if (!address || typeof address === 'string') throw Error('Expected a test TCP listener');
  return new URL(`http://127.0.0.1:${address.port}`);
}

async function close(server: ReturnType<typeof createServer>) {
  await new Promise<void>((resolve, reject) => {
    server.close(error => error ? reject(error) : resolve());
    server.closeAllConnections();
  });
}

it('forwards the native reply and binds commands to the verified selection', async () => {
  const commands: Array<{method?: string; path?: string; user?: string | string[]; body: string}> = [];
  const owner = createServer((req, res) => {
    if (req.url?.startsWith('/internal/session-context?')) {
      res.setHeader('content-type', 'application/json');
      res.end(JSON.stringify({ data: actorContext }));
      return;
    }
    let body = '';
    req.setEncoding('utf8').on('data', chunk => { body += chunk; });
    req.on('end', () => {
      commands.push({ method: req.method, path: req.url, user: req.headers['x-erpx-context-user'], body });
      res.writeHead(200, { 'content-type': 'application/json', 'cache-control': 'private, no-store' });
      res.end(JSON.stringify({ data: { id: 'rec-1' }, context: actorContext }));
    });
  });
  const base = await listen(owner);
  const gateway = createServer((req, res) => void proxyNativeApi({ base, req, res, recordAction: true }));
  try {
    const ingress = await listen(gateway);
    const headers = { cookie: 'session=fixture', 'x-erpx-context-user': 'untrusted-user' };
    const response = await fetch(new URL(endpoint + '?view=mention', ingress), { headers });
    expect(response.headers.get('cache-control')).toBe('private, no-store');
    expect(await response.json()).toEqual({ data: { id: 'rec-1' }, context: actorContext });

    const stale = await fetch(new URL(endpoint, ingress), {
      method: 'PATCH', headers: { ...headers, 'x-erpx-context-revision': '6' }, body: '{}',
    });
    expect(stale.status).toBe(409);
    expect(await stale.json()).toMatchObject({ error: { code: 'WORK_CONTEXT_CHANGED' } });

    for (const [method, suffix, body] of [
      ['PATCH', '?ignored=1', '{}'],
      ['POST', '/references', '{}'],
      ['POST', '/transition', '{}'],
      ['DELETE', '?expectedVersion=2', undefined],
    ] as const) {
      const result = await fetch(new URL(endpoint + suffix, ingress), {
        method, headers: { ...headers, 'x-erpx-context-revision': '7' }, body,
      });
      expect(result.status).toBe(200);
      await result.arrayBuffer();
    }

    expect(commands).toEqual([
      { method: 'GET', path: endpoint, user: 'current-user', body: '' },
      { method: 'PATCH', path: endpoint, user: 'current-user', body: '{}' },
      { method: 'POST', path: endpoint + '/references', user: 'current-user', body: '{}' },
      { method: 'POST', path: endpoint + '/transition', user: 'current-user', body: '{}' },
      { method: 'DELETE', path: endpoint + '?expectedVersion=2', user: 'current-user', body: '' },
    ]);

    const unbounded = await fetch(new URL(endpoint + '?q=' + 'x'.repeat(4096), ingress), { headers });
    expect(unbounded.status).toBe(422);
    await unbounded.arrayBuffer();
    expect(commands).toHaveLength(5);

    await close(owner);
    const unavailable = await fetch(new URL(endpoint, ingress), { method: 'DELETE' });
    expect(unavailable.status).toBe(503);
    expect(await unavailable.json()).toMatchObject({ error: { code: 'NATIVE_RECORD_REGISTRY_UNAVAILABLE' } });
    expect(commands).toHaveLength(5);
  } finally {
    await close(gateway);
    if (owner.listening) await close(owner);
  }
});
