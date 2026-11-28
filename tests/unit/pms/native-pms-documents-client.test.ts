import { readFile } from 'node:fs/promises';
import { describe, expect, it, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  PMS_DOCUMENTS_CONFIG_KEY,
  createPmsDocumentsClient,
  pmsDocumentsConfig,
  pmsDocumentsConfigured,
  type PmsDocumentActor,
  type PmsDocumentsTransport,
} from '../../../modules/pms/ui/src/adapters/server/native-pms-documents';

const raw = JSON.stringify({
  privateBaseUrl: 'http://127.0.0.1:44993',
  timeoutMs: 1000,
  holdings: [{ origin: 'https://acme.jesadakorn.com', holdingSlug: 'acme', holdingId: 'h001' }],
});
const cfg = pmsDocumentsConfig({ [PMS_DOCUMENTS_CONFIG_KEY]: raw });
const actor: PmsDocumentActor = { holdingId: 'h001', holdingSlug: 'acme', companyId: 'company-1' };
const row = {
  id: 'doc-1',
  property_id: 'property-1',
  guest_account_id: 'guest-1',
  kind: 'receipt',
  finance_id: 'finance-1',
  snapshot_json: '{"amount":100}',
  created_at: 99,
};
const response = (body: unknown, status = 200): Response =>
  new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });

describe('strict native PMS document client', () => {
  it('reads the full document payload from the mounted owner route', async () => {
    const seen: URL[] = [];
    const headers: Record<string, string>[] = [];
    const transport: PmsDocumentsTransport = async (url, h) => {
      seen.push(url);
      headers.push({ ...h });
      return response({ document: row });
    };
    const doc = await createPmsDocumentsClient(cfg, transport)(actor).document('doc-1');
    expect(doc).toEqual(row);
    expect(seen[0]?.pathname).toBe('/api/v1/holdings/acme/pms/native-documents/document');
    expect(seen[0]?.searchParams.get('id')).toBe('doc-1');
    expect(headers[0]).toMatchObject({
      host: 'acme.jesadakorn.com',
      origin: 'https://acme.jesadakorn.com',
      'x-erpx-context-company': 'company-1',
    });
  });

  it('reads the full related-postings payload from the mounted owner route', async () => {
    const seen: URL[] = [];
    const transport: PmsDocumentsTransport = async (url) => {
      seen.push(url);
      return response({ postings: [{ journal_id: 'j-1' }, { journal_id: 'j-2' }] });
    };
    const postings = await createPmsDocumentsClient(cfg, transport)(actor).relatedPostings('property-1', 'doc-1');
    expect(postings).toEqual([{ journal_id: 'j-1' }, { journal_id: 'j-2' }]);
    expect(seen[0]?.pathname).toBe('/api/v1/holdings/acme/pms/native-documents/postings');
    expect(seen[0]?.searchParams.get('property')).toBe('property-1');
    expect(seen[0]?.searchParams.get('id')).toBe('doc-1');
  });

  it('maps an absent document to null and an absent posting list to empty', async () => {
    const transport: PmsDocumentsTransport = async () => response({ document: null });
    const queries = createPmsDocumentsClient(cfg, transport)(actor);
    expect(await queries.document('doc-1')).toBeNull();
    const postingsTransport: PmsDocumentsTransport = async () => response({ postings: null });
    expect(await createPmsDocumentsClient(cfg, postingsTransport)(actor).relatedPostings('p', 'd')).toEqual([]);
  });

  it('fails closed with no config, no matching target and no fallback', async () => {
    expect(pmsDocumentsConfigured({})).toBe(false);
    expect(() => pmsDocumentsConfig({})).toThrowError(/not configured/);
    let calls = 0;
    const transport: PmsDocumentsTransport = async () => {
      calls += 1;
      return response({ document: row });
    };
    expect(() =>
      createPmsDocumentsClient(cfg, transport)({
        ...actor,
        holdingId: 'h-other',
      }),
    ).toThrowError(/not registered/);
    expect(calls).toBe(0);
  });

  it('rejects incomplete payloads and preserves native 4xx errors', async () => {
    const partial = createPmsDocumentsClient(cfg, async () => response({ document: { id: 'doc-1' } }))(actor);
    await expect(partial.document('doc-1')).rejects.toMatchObject({
      code: 'PMS_NATIVE_DOCUMENTS_UNAVAILABLE',
      status: 503,
    });
    const rejected = createPmsDocumentsClient(
      cfg,
      async () => response({ error: { code: 'PMS_NATIVE_DOCUMENTS_REQUEST_INVALID', message: 'bounded' } }, 422),
    )(actor);
    await expect(rejected.document('bad id')).rejects.toMatchObject({
      code: 'PMS_NATIVE_DOCUMENTS_UNAVAILABLE',
      status: 503,
    });
    await expect(rejected.document('doc-1')).rejects.toMatchObject({
      code: 'PMS_NATIVE_DOCUMENTS_REQUEST_INVALID',
      status: 422,
    });
  });

  it('carries no SQL, no DB unit and no TypeScript fallback reader', async () => {
    const source = await readFile(
      new URL('../../../modules/pms/ui/src/adapters/server/native-pms-documents.ts', import.meta.url),
      'utf8',
    );
    expect(source).not.toMatch(/\bSELECT\b|\bdb\.prepare\b|nativePmsReadAll|nativePmsReadFirst/);
    expect(source).toContain('native-documents/document');
    expect(source).toContain('native-documents/postings');
  });
});
