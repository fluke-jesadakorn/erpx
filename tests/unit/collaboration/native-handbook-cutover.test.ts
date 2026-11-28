import { existsSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it, vi } from 'vitest';
import { holdingHosts } from '../../../platform/contracts/src/holding-hosts';
import { expandNativeApiPaths, loadNativeApiPaths } from '../../../scripts/local/native-api-paths';
import { nativeApiPath } from '../../../scripts/local/native-api-proxy';
vi.mock('server-only', () => ({}));
import {
  createNativeHandbookClient,
  type HandbookTransport,
} from '../../../modules/collaboration/ui/src/adapters/server/native-handbook';

const repoFile = (path: string) => fileURLToPath(new URL(`../../../${path}`, import.meta.url));
const exists = (path: string) => existsSync(repoFile(path));
const read = (path: string) => readFileSync(repoFile(path), 'utf8');
const checkedIn = loadNativeApiPaths(repoFile('scripts/local/native-api-paths.json'));
const environment = {
  NODE_ENV: 'test' as const,
  ERPX_DEV_NATIVE_PATHS: expandNativeApiPaths(checkedIn, holdingHosts).paths,
};
const config = JSON.stringify({
  privateBaseUrl: 'http://127.0.0.1:3105',
  timeoutMs: 1000,
  holdings: [{ origin: 'https://acme.example.test', holdingSlug: 'acme', holdingId: 'h001' }],
});
const incoming = new Headers({
  host: 'acme.example.test',
  cookie: '__Host-erpx-product=session',
  origin: 'https://acme.example.test',
  'x-erpx-context-company': 'company',
  'x-erpx-context-assignment': 'handbook-assignment',
});
function payload(body: unknown, status = 200) {
  const calls: { url?: URL; method?: string; key?: string | null }[] = [];
  const handler: HandbookTransport = async (url, init) => {
    calls.push({ url, method: init.method, key: init.headers.get('idempotency-key') });
    return new Response(JSON.stringify({ data: body }), {
      status,
      headers: { 'content-type': 'application/json' },
    });
  };
  return { calls, handler };
}

describe('handbook Next catch-all cutover', () => {
  it('has no shared Next handler and no launcher/compat re-export left', () => {
    for (const file of [
      'products/apps/ui/src/routes/api/v1/holdings/[holdingSlug]/handbook/[[...path]]/route.ts',
      'apps/launcher/src/app/api/v1/holdings/[holdingSlug]/handbook/[[...path]]/route.ts',
      'apps/compat/src/app/api/v1/holdings/[holdingSlug]/handbook/[[...path]]/route.ts',
    ]) expect(exists(file), file).toBe(false);
  });

  it('routes every served handbook subpath and method through the exact holding prefix', () => {
    expect(checkedIn.holdingPrefixes).toContain('handbook');
    for (const { hostname, holdingSlug } of holdingHosts) {
      const base = `/api/v1/holdings/${holdingSlug}/handbook`;
      expect(environment.ERPX_DEV_NATIVE_PATHS).toContain(`${hostname}|${base}`);
      for (const [path, method] of [
        [base, 'GET'], [base, 'POST'],
        [`${base}/editors`, 'GET'], [`${base}/editors`, 'POST'],
        [`${base}/article-1`, 'GET'], [`${base}/article-1`, 'POST'],
        [`${base}/article-1/actions`, 'POST'],
      ]) expect(nativeApiPath(environment, path, hostname, method), `${hostname} ${method} ${path}`).toBe(true);
      expect(nativeApiPath(environment, base, 'unregistered.example.test', 'GET')).toBe(false);
      expect(nativeApiPath(environment, '/api/v1/holdings/' + holdingSlug + '/handbook-x', hostname, 'GET')).toBe(false);
    }
  });

  it('maps all four native owner routes with their exact method handlers', () => {
    const source = read('modules/collaboration/server/src/adapters/handbook_http.rs');
    for (const path of [
      '/api/v1/holdings/{holding_slug}/handbook',
      '/api/v1/holdings/{holding_slug}/handbook/editors',
      '/api/v1/holdings/{holding_slug}/handbook/{record_id}',
      '/api/v1/holdings/{holding_slug}/handbook/{record_id}/actions',
    ]) expect(source, path).toContain(`"${path}"`);
    expect(source).toContain('get(read_list).post(create_article)');
    expect(source).toContain('get(read_editors).post(set_editor)');
    expect(source).toContain('get(read_article).post(update_article)');
    expect(source).toContain('post(article_action)');
    expect(source).toContain('Json(json!({ "data": body }))');
    expect(source).toContain('Json(json!({"error":{"code":error.code,"message":error.message}}))');
  });

  it('keeps the owner SQL and bounded envelope as the native payload authority', () => {
    const sql = read('modules/collaboration/server/src/adapters/handbook.rs');
    expect(sql).toContain('sqlx::query(');
    expect(sql).toContain('INSERT INTO handbook_articles');
    expect(sql).toContain('INSERT INTO handbook_revisions');
    expect(sql).toContain('UPDATE handbook_articles');
    expect(sql).toContain('UPDATE handbook_revisions');
  });

  it('returns the native {data} payload for every served subpath and method', async () => {
    const client = createNativeHandbookClient(config, payload({ ok: true }).handler);
    const list = payload({ access: { editor: true }, articles: [] });
    await expect(createNativeHandbookClient(config, list.handler).list(incoming, 'acme')).resolves.toMatchObject({ access: { editor: true } });
    expect(list.calls[0].url?.href).toBe('http://127.0.0.1:3105/api/v1/holdings/acme/handbook');
    expect(list.calls[0].method).toBe('GET');

    const article = payload({ article: { id: 'article-1' } });
    await expect(createNativeHandbookClient(config, article.handler).article(incoming, 'acme', 'article-1', { edit: true, revision: 2 })).resolves.toMatchObject({ article: { id: 'article-1' } });
    expect(article.calls[0].url?.href).toBe('http://127.0.0.1:3105/api/v1/holdings/acme/handbook/article-1?edit=1&revision=2');

    const editors = payload({ version: 4, editors: [] });
    await expect(createNativeHandbookClient(config, editors.handler).editors(incoming, 'acme')).resolves.toMatchObject({ version: 4 });
    expect(editors.calls[0].url?.href).toBe('http://127.0.0.1:3105/api/v1/holdings/acme/handbook/editors');

    for (const [path, key] of [['', 'handbook-create-0001'], ['/editors', 'handbook-editor-0001'], ['/article-1/actions', 'handbook-action-0001']] as const) {
      const command = payload({ id: 'receipt-1' });
      await expect(createNativeHandbookClient(config, command.handler).command(incoming, 'acme', path, '{}', key)).resolves.toMatchObject({ id: 'receipt-1' });
      expect(command.calls[0].method).toBe('POST');
      expect(command.calls[0].url?.href).toBe(`http://127.0.0.1:3105/api/v1/holdings/acme/handbook${path}`);
      expect(command.calls[0].key).toBe(key);
    }
    expect(client).toBeDefined();
  });
});
