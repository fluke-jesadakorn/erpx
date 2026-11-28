import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { createHash } from 'node:crypto';
import { createServer, type IncomingMessage, type Server } from 'node:http';
import fs from 'node:fs';
import path from 'node:path';

vi.mock('server-only', () => ({}));

import {
  nativeExecutedCopiesRead,
  nativeLane26Configured,
  nativeLane26Scope,
  nativeLegalCitationChunk,
  nativeLegalTextApproved,
  nativeLegalTrustApprovedMaterials,
  nativeLegalTrustMaterial,
} from '../../../modules/legal/server/src/adapters/native-lane-26/client';
import type { NativeExecutedCopyRead } from '../../../modules/legal/server/src/adapters/native-lane-26/client';

const root = process.cwd();
const ENV = 'ERPX_NATIVE_LEGAL_LANE26_CONFIG';
const sha = (value: string) => createHash('sha256').update(value).digest('hex');

const RETIRED_SQL = [
  'SELECT e.*,d.name document_name,d.version document_version FROM legal_executed_copies e JOIN legal_documents d ON d.id=e.document_id WHERE e.item_id=? ORDER BY e.imported_at DESC,e.id LIMIT 100',
  'SELECT * FROM legal_executed_reviews WHERE executed_copy_id=? ORDER BY created_at,id',
  'SELECT 1 FROM legal_members WHERE item_id=? AND user_id=?',
  'SELECT e.*,d.created_by document_author,p.created_by approved_author FROM legal_executed_copies e JOIN legal_documents d ON d.id=e.document_id JOIN legal_documents p ON p.id=e.approved_document_id WHERE e.id=? AND e.item_id=?',
  "SELECT cycle FROM legal_approvals WHERE item_id=? AND decision='approved' ORDER BY cycle DESC LIMIT 1",
];
const RETIRED_SQL_ORACLES = [
  '6f0594b391932e20815bcb735e16f707fb303e8d55f35fe35aabe25e96fe3a6a',
  'abb6ba89de9eb9a16001bcdf2689a0fb3ba332a4b0c9410c0ff9ac632c57ef15',
  '72a0b27c824165888c9d0cf61784a424075b328b8d494860350f777d78c3b952',
  'af5056bdcf410a5e12a1c577ac14c2169a389ff9c6ada34c79db83f8cb14c492',
  '8a785ee052ae40ce25bd05a5f5decb3f74ea84efd9c04bc818ab45f806da59e9',
];
const RETIRED_SQL_JOINED_ORACLE = 'b0b05ddc3a2aef19e2b39f0b673bf5e1e4793bab53e26fbf707d3bc0a13f535b';

const TRUST_SQL = [
  "SELECT id,kind,content,checksum FROM legal_trust_material WHERE company_id=? AND state='approved' ORDER BY id LIMIT 100",
  'SELECT * FROM legal_trust_material WHERE id=? AND company_id=?',
];
const TRUST_SQL_ORACLES = [
  '468464247af969cbb01eb134e203a18e37b0c274823a9b5e95049e5167025b62',
  '6bced34abbe5d52f5e635d90186fb1f03ce0508f2b402d1cc8da63b426c81950',
];
const TRUST_SQL_JOINED_ORACLE = 'ff3d835731f68fd4ed30084edce2cc135f3bd9694463acfadc94505b87ecc9e6';
const TRUST_MATERIAL = { id: 'tm-1', kind: 'root', content: 'base64', checksum: 'hash' };
const TRUST_DETAIL = { id: 'tm-1', state: 'approved', version: 2, created_by: 'user-1' };
const APPROVED_ORACLE = 'aa68e4a64feba27173de3958d4a7ed9c1fa849d1b8e53e4b57e41b3226501a0c';
const DETAIL_ORACLE = 'cd633e5af71aed5a7e82d05bb1737dd2787551ea8aa86f19516e556fe7207645';

const CITATION_SQL = [
  "SELECT c.id,c.document_id,CASE WHEN c.page_mapping THEN c.page ELSE NULL END page,c.page_mapping,c.section,c.text,c.document_checksum FROM legal_source_chunks c JOIN legal_text_reviews r ON r.id=c.text_review_id AND r.state='approved' AND r.company_id=c.company_id AND r.item_id=c.item_id AND r.document_id=c.document_id AND r.document_checksum=c.document_checksum WHERE c.id=? AND c.company_id=?",
  "SELECT 1 FROM legal_text_reviews WHERE id=? AND document_id=? AND state='approved'",
];
const CITATION_SQL_ORACLES = [
  '8a5dab616d56323ba8abdb3682b1722845c7b49dc2aed404c2a778406e7609df',
  'f6e2085835219b43c8e7e74fba4abefbac94f3fbd41aa3847b1490fa399f7104',
];
const CITATION_CHUNK = {
  id: 'chunk-1',
  document_id: 'doc-1',
  page: 3,
  page_mapping: true,
  section: 'section',
  text: 'text',
  document_checksum: 'checksum',
};
const CHUNK_ORACLE = '713d0760c45e1ae9f77d0c2b17a26610b31827e5ff8934c3c56a9aab96cdcefd';
const TEXT_APPROVED_ORACLE = '20ec2f38fc96bc7d416a9af537493ae0bae26c5ad4c5a0f2d94f56f9c3e04958';

const read: NativeExecutedCopyRead = {
  item_id: 'item-1',
  member: true,
  approval: { cycle: 3 },
  copies: [
    {
      id: 'copy-1',
      item_id: 'item-1',
      document_id: 'doc-1',
      approved_cycle: 3,
      approved_document_id: 'doc-0',
      approved_hash: 'aa',
      executed_hash: 'bb',
      metadata: { pages: 2 },
      state: 'pending_review',
      imported_by: 'importer',
      imported_at: 10,
      reviewed_by: null,
      reviewed_at: null,
      version: 1,
      document_name: 'agreement.pdf',
      document_version: 2,
      document_author: 'author',
      approved_author: 'approver',
      reviews: [
        {
          id: 'review-1',
          executed_copy_id: 'copy-1',
          item_version: 1,
          decision: 'confirm',
          checks: { contentMatches: true },
          reason: 'ok',
          reviewer_id: 'reviewer',
          created_at: 11,
        },
      ],
    },
  ],
};
const READ_ORACLE = 'b773852dcfb0b1734452f2e66e60924fe57f82960ccfd8d5bd4e642707d6bd5d';

type Seen = {
  url: string;
  host: string | undefined;
  origin: string | undefined;
  company: string | undefined;
  user: string | undefined;
};
let server: Server;
let requests: Seen[];
let port: number;
let mode: 'ok' | 'bad' | 'down' = 'ok';
let original: string | undefined;

const HOLDINGS: readonly {
  readonly origin: string;
  readonly host: string;
  readonly holdingId: string;
  readonly holdingSlug: string;
  readonly companyId?: string;
}[] = [
  {
    origin: 'https://acme.jesadakorn.com',
    host: 'acme.jesadakorn.com',
    holdingId: 'h001',
    holdingSlug: 'acme',
    companyId: 'acme-co',
  },
  {
    origin: 'https://northstar.jesadakorn.com',
    host: 'northstar.jesadakorn.com',
    holdingId: 'h002',
    holdingSlug: 'northstar',
  },
  {
    origin: 'https://erpx-internal.jesadakorn.com',
    host: 'erpx-internal.jesadakorn.com',
    holdingId: 'h003',
    holdingSlug: 'erpx',
  },
] as const;

/** Exact development value for ERPX_NATIVE_LEGAL_LANE26_CONFIG. */
const DEV_CONFIG = `{
  "privateBaseUrl": "http://127.0.0.1:3198",
  "timeoutMs": 10000,
  "holdings": [
    {
      "origin": "https://acme.jesadakorn.com",
      "host": "acme.jesadakorn.com",
      "holdingId": "h001",
      "holdingSlug": "acme"
    },
    {
      "origin": "https://northstar.jesadakorn.com",
      "host": "northstar.jesadakorn.com",
      "holdingId": "h002",
      "holdingSlug": "northstar"
    },
    {
      "origin": "https://erpx-internal.jesadakorn.com",
      "host": "erpx-internal.jesadakorn.com",
      "holdingId": "h003",
      "holdingSlug": "erpx"
    }
  ]
}`;

const scope = nativeLane26Scope({
  holdingId: 'h001',
  holdingSlug: 'acme',
  companyId: 'acme-co',
  userId: 'user-1',
});

const respond = (res: import('node:http').ServerResponse, body: unknown, status = 200) => {
  res.writeHead(status, { 'content-type': 'application/json' });
  res.end(JSON.stringify(body));
};

beforeAll(async () => {
  requests = [];
  server = createServer((req: IncomingMessage, res) => {
    const url = new URL(req.url ?? '/', 'http://127.0.0.1');
    requests.push({
      url: url.pathname,
      host: req.headers.host,
      origin: req.headers.origin,
      company: req.headers['x-erpx-context-company'] as string | undefined,
      user: req.headers['x-erpx-context-user'] as string | undefined,
    });
    if (mode === 'down') return respond(res, { error: { code: 'DOWN', message: 'down' } }, 503);
    if (url.pathname.includes('/trust-materials/approved'))
      return respond(res, mode === 'bad' ? { materials: [{ id: 'only-id' }] } : { materials: [TRUST_MATERIAL] });
    if (url.pathname.includes('/trust-materials/'))
      return respond(res, mode === 'bad' ? { material: { id: 'only-id' } } : { material: TRUST_DETAIL });
    if (url.pathname.includes('/citation-chunk/'))
      return respond(res, mode === 'bad' ? { chunk: { id: 'only-id' } } : { chunk: CITATION_CHUNK });
    if (url.pathname.includes('/text-approved/'))
      return respond(res, mode === 'bad' ? { approved: 'yes' } : { approved: true });
    if (mode === 'bad') return respond(res, { ...read, copies: [{ id: 'copy-1' }] });
    return respond(res, read);
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address();
  if (!address || typeof address === 'string') throw new Error('no server port');
  port = address.port;
});

afterAll(async () => {
  await new Promise<void>(resolve => server.close(() => resolve()));
});

beforeEach(() => {
  mode = 'ok';
  requests = [];
  original = process.env[ENV];
  process.env[ENV] = JSON.stringify({
    privateBaseUrl: `http://127.0.0.1:${port}`,
    timeoutMs: 1000,
    holdings: HOLDINGS,
  });
});

afterEach(() => {
  if (original === undefined) delete process.env[ENV];
  else process.env[ENV] = original;
});

describe('native Legal lane 26 executed-copy reads', () => {
  it('accepts all three registered Holdings in the exact development config', async () => {
    process.env[ENV] = DEV_CONFIG;
    expect(nativeLane26Configured()).toBe(true);
    // An unregistered actor fails before any request, proving the config parsed
    // and that no Holding is used as a default.
    await expect(
      nativeExecutedCopiesRead(
        { holdingId: 'h004', holdingSlug: 'unknown', companyId: 'acme-co', userId: 'user-1' },
        'item-1',
      ),
    ).rejects.toMatchObject({
      status: 503,
      code: 'NATIVE_LEGAL_COPY_AUTHORITY_INVALID',
    });
    expect(requests).toHaveLength(0);
  });

  it('pins the retired owner SQL and response projection oracles', () => {
    expect(RETIRED_SQL.map(sha)).toEqual(RETIRED_SQL_ORACLES);
    expect(sha(RETIRED_SQL.join('\n'))).toBe(RETIRED_SQL_JOINED_ORACLE);
    expect(sha(JSON.stringify(read))).toBe(READ_ORACLE);
    expect(TRUST_SQL.map(sha)).toEqual(TRUST_SQL_ORACLES);
    expect(sha(TRUST_SQL.join('\n'))).toBe(TRUST_SQL_JOINED_ORACLE);
    expect(sha(JSON.stringify({ materials: [TRUST_MATERIAL] }))).toBe(APPROVED_ORACLE);
    expect(sha(JSON.stringify({ material: TRUST_DETAIL }))).toBe(DETAIL_ORACLE);
    expect(CITATION_SQL.map(sha)).toEqual(CITATION_SQL_ORACLES);
    expect(sha(JSON.stringify({ chunk: CITATION_CHUNK }))).toBe(CHUNK_ORACLE);
    expect(sha(JSON.stringify({ approved: true }))).toBe(TEXT_APPROVED_ORACLE);
  });

  it('delegates both trust-material reads to native routes with isolated headers', async () => {
    const approved = await nativeLegalTrustApprovedMaterials(scope);
    expect(approved).toEqual([TRUST_MATERIAL]);
    expect(requests[0].url).toBe(
      '/api/v1/holdings/acme/legal/native-lane-26/trust-materials/approved',
    );
    expect(requests[0].origin).toBe('https://acme.jesadakorn.com');
    expect(requests[0].company).toBe('acme-co');
    requests = [];
    const material = await nativeLegalTrustMaterial(scope, 'tm-1');
    expect(material).toEqual(TRUST_DETAIL);
    expect(requests[0].url).toBe(
      '/api/v1/holdings/acme/legal/native-lane-26/trust-materials/tm-1',
    );
    expect(requests[0].user).toBe('user-1');
  });

  it('selects trust material per Holding and never defaults to the first', async () => {
    for (const holding of HOLDINGS) {
      requests = [];
      const actorCompany = holding.companyId ?? `company-for-${holding.holdingId}`;
      const actor = {
        holdingId: holding.holdingId,
        holdingSlug: holding.holdingSlug,
        companyId: actorCompany,
        userId: 'user-1',
      };
      expect(await nativeLegalTrustApprovedMaterials(actor)).toEqual([TRUST_MATERIAL]);
      expect(requests[0].url).toBe(
        `/api/v1/holdings/${holding.holdingSlug}/legal/native-lane-26/trust-materials/approved`,
      );
      expect(requests[0].origin).toBe(holding.origin);
    }
    requests = [];
    await expect(
      nativeLegalTrustMaterial(
        { ...scope, holdingSlug: 'unknown' },
        'tm-1',
      ),
    ).rejects.toMatchObject({ status: 503 });
    expect(requests).toHaveLength(0);
  });

  it('fails closed when the trust read is down, malformed or unbounded', async () => {
    mode = 'down';
    await expect(nativeLegalTrustApprovedMaterials(scope)).rejects.toMatchObject({ status: 503 });
    mode = 'bad';
    await expect(nativeLegalTrustApprovedMaterials(scope)).rejects.toMatchObject({
      status: 503,
      code: 'NATIVE_LEGAL_COPY_READ_RESPONSE_INVALID',
    });
    await expect(nativeLegalTrustMaterial(scope, 'tm-1')).rejects.toMatchObject({ status: 503 });
    await expect(nativeLegalTrustMaterial(scope, 'bad id!')).rejects.toMatchObject({ status: 503 });
  });

  it('delegates the complete projection to the native route with isolated headers', async () => {
    expect(nativeLane26Configured()).toBe(true);
    const result = await nativeExecutedCopiesRead(scope, 'item-1');
    expect(result).toEqual(read);
    expect(requests).toHaveLength(1);
    expect(requests[0].url).toBe(
      '/api/v1/holdings/acme/legal/native-lane-26/executed-copies/item-1',
    );
    expect(requests[0].origin).toBe('https://acme.jesadakorn.com');
    expect(requests[0].company).toBe('acme-co');
    expect(requests[0].user).toBe('user-1');
  });

  it('selects every registered Holding from the caller WorkContext', async () => {
    for (const holding of HOLDINGS) {
      requests = [];
      // Only h001 pins a company in this config; the other actors carry their
      // own signed-in Company, which the transport forwards unchanged.
      const actorCompany = holding.companyId ?? `company-for-${holding.holdingId}`;
      const result = await nativeExecutedCopiesRead(
        {
          holdingId: holding.holdingId,
          holdingSlug: holding.holdingSlug,
          companyId: actorCompany,
          userId: 'user-1',
        },
        'item-1',
      );
      expect(result).toEqual(read);
      expect(requests).toHaveLength(1);
      expect(requests[0].url).toBe(
        `/api/v1/holdings/${holding.holdingSlug}/legal/native-lane-26/executed-copies/item-1`,
      );
      expect(requests[0].origin).toBe(holding.origin);
      expect(requests[0].company).toBe(actorCompany);
    }
  });

  it('never defaults to the first Holding for an unknown actor', async () => {
    await expect(
      nativeExecutedCopiesRead({ ...scope, holdingSlug: 'unknown' }, 'item-1'),
    ).rejects.toMatchObject({ status: 503 });
    await expect(
      nativeExecutedCopiesRead({ ...scope, companyId: 'wrong-company' }, 'item-1'),
    ).rejects.toMatchObject({ status: 503 });
    await expect(
      nativeExecutedCopiesRead({ ...scope, holdingId: '' }, 'item-1'),
    ).rejects.toMatchObject({ status: 503 });
    expect(requests).toHaveLength(0);
  });

  it('fails closed for malformed multi-Holding configuration', async () => {
    const set = (holdings: unknown) =>
      (process.env[ENV] = JSON.stringify({
        privateBaseUrl: `http://127.0.0.1:${port}`,
        timeoutMs: 1000,
        holdings,
      }));
    set([{ origin: 'https://acme.jesadakorn.com', holdingId: 'h001', holdingSlug: 'acme' }]);
    await expect(nativeExecutedCopiesRead(scope, 'item-1')).rejects.toMatchObject({ status: 503 });
    set([
      {
        origin: 'https://acme.jesadakorn.com',
        host: 'wrong.example.com',
        holdingId: 'h001',
        holdingSlug: 'acme',
      },
    ]);
    await expect(nativeExecutedCopiesRead(scope, 'item-1')).rejects.toMatchObject({ status: 503 });
    set([HOLDINGS[0], HOLDINGS[0]]);
    await expect(nativeExecutedCopiesRead(scope, 'item-1')).rejects.toMatchObject({ status: 503 });
    set(
      Array.from({ length: 65 }, (_, index) => ({
        origin: `https://h${index}.jesadakorn.com`,
        host: `h${index}.jesadakorn.com`,
        holdingId: `holding-${index}`,
        holdingSlug: `h${index}`,
      })),
    );
    await expect(nativeExecutedCopiesRead(scope, 'item-1')).rejects.toMatchObject({ status: 503 });
    expect(requests).toHaveLength(0);
  });

  it('fails closed when configuration is absent or malformed', async () => {
    delete process.env[ENV];
    expect(nativeLane26Configured()).toBe(false);
    await expect(nativeExecutedCopiesRead(scope, 'item-1')).rejects.toMatchObject({
      status: 503,
      code: 'NATIVE_LEGAL_COPY_READ_CONFIGURATION_REQUIRED',
    });
    process.env[ENV] = 'not-json';
    await expect(nativeExecutedCopiesRead(scope, 'item-1')).rejects.toMatchObject({ status: 503 });
    expect(requests).toHaveLength(0);
  });

  it('fails closed for an unregistered target or an unbounded item id', async () => {
    await expect(
      nativeExecutedCopiesRead({ ...scope, holdingId: 'another' }, 'item-1'),
    ).rejects.toMatchObject({ status: 503 });
    await expect(nativeExecutedCopiesRead(scope, 'bad id!')).rejects.toMatchObject({ status: 503 });
    expect(requests).toHaveLength(0);
  });

  it('fails closed when the native service is down or the payload is malformed', async () => {
    mode = 'down';
    await expect(nativeExecutedCopiesRead(scope, 'item-1')).rejects.toMatchObject({ status: 503 });
    mode = 'bad';
    await expect(nativeExecutedCopiesRead(scope, 'item-1')).rejects.toMatchObject({
      status: 503,
      code: 'NATIVE_LEGAL_COPY_READ_RESPONSE_INVALID',
    });
  });

  it('delegates both citation-evidence reads to native routes with isolated headers', async () => {
    const chunk = await nativeLegalCitationChunk(scope, 'chunk-1');
    expect(chunk).toEqual(CITATION_CHUNK);
    expect(requests[0].url).toBe(
      '/api/v1/holdings/acme/legal/native-lane-26/citation-chunk/chunk-1',
    );
    expect(requests[0].origin).toBe('https://acme.jesadakorn.com');
    expect(requests[0].company).toBe('acme-co');
    requests = [];
    expect(await nativeLegalTextApproved(scope, 'review-1', 'doc-1')).toBe(true);
    expect(requests[0].url).toBe(
      '/api/v1/holdings/acme/legal/native-lane-26/text-approved/review-1/doc-1',
    );
    expect(requests[0].user).toBe('user-1');
  });

  it('selects citation evidence per Holding and never defaults to the first', async () => {
    for (const holding of HOLDINGS) {
      requests = [];
      const actorCompany = holding.companyId ?? `company-for-${holding.holdingId}`;
      const actor = {
        holdingId: holding.holdingId,
        holdingSlug: holding.holdingSlug,
        companyId: actorCompany,
        userId: 'user-1',
      };
      expect(await nativeLegalCitationChunk(actor, 'chunk-1')).toEqual(CITATION_CHUNK);
      expect(requests[0].url).toBe(
        `/api/v1/holdings/${holding.holdingSlug}/legal/native-lane-26/citation-chunk/chunk-1`,
      );
      expect(requests[0].origin).toBe(holding.origin);
      expect(requests[0].company).toBe(actorCompany);
    }
    requests = [];
    await expect(
      nativeLegalCitationChunk({ ...scope, holdingSlug: 'unknown' }, 'chunk-1'),
    ).rejects.toMatchObject({ status: 503 });
    expect(requests).toHaveLength(0);
  });

  it('fails closed when citation reads are down, malformed or unbounded', async () => {
    mode = 'down';
    await expect(nativeLegalCitationChunk(scope, 'chunk-1')).rejects.toMatchObject({ status: 503 });
    await expect(nativeLegalTextApproved(scope, 'review-1', 'doc-1')).rejects.toMatchObject({
      status: 503,
    });
    mode = 'bad';
    await expect(nativeLegalCitationChunk(scope, 'chunk-1')).rejects.toMatchObject({
      status: 503,
      code: 'NATIVE_LEGAL_COPY_READ_RESPONSE_INVALID',
    });
    await expect(nativeLegalTextApproved(scope, 'review-1', 'doc-1')).rejects.toMatchObject({
      status: 503,
      code: 'NATIVE_LEGAL_COPY_READ_RESPONSE_INVALID',
    });
    await expect(nativeLegalCitationChunk(scope, 'bad id!')).rejects.toMatchObject({ status: 503 });
    await expect(nativeLegalTextApproved(scope, 'bad id!', 'doc-1')).rejects.toMatchObject({
      status: 503,
    });
    await expect(nativeLegalTextApproved(scope, 'review-1', 'bad id!')).rejects.toMatchObject({
      status: 503,
    });
  });

  it('removed all seven owner statements from the TypeScript read source', () => {
    const source = fs.readFileSync(
      path.join(root, 'modules/legal/server/src/adapters/legal/executed-copies.ts'),
      'utf8',
    );
    for (const sql of RETIRED_SQL) expect(source).not.toContain(sql);
    expect(source).not.toMatch(/\.prepare\(/);
    expect(source).not.toMatch(/legalOrderedHistorySql|legalApprovedCycleSql|legalMembershipSql/);
    expect(source).toContain("from '../native-lane-26/client'");

    const validation = fs.readFileSync(
      path.join(root, 'modules/legal/server/src/adapters/legal/pdf-validation.ts'),
      'utf8',
    );
    expect(validation).not.toContain(TRUST_SQL[0]);
    expect(validation).not.toMatch(/\.prepare\(/);
    expect(validation).toContain("from '../native-lane-26/client'");

    const trust = fs.readFileSync(
      path.join(root, 'modules/legal/server/src/adapters/legal/trust.ts'),
      'utf8',
    );
    expect(trust).not.toContain(TRUST_SQL[1]);
    expect(trust).not.toContain('legalByIdAndCompanySql');
    expect(trust).toContain("from '../native-lane-26/client'");

    const citation = fs.readFileSync(
      path.join(root, 'modules/legal/server/src/adapters/legal/citations.ts'),
      'utf8',
    );
    expect(citation).not.toContain(CITATION_SQL[0]);
    expect(citation).not.toContain('legal_source_chunks');
    // Only the Files availability witness stays owner-local in TypeScript.
    expect((citation.match(/\.prepare\(/g) ?? []).length).toBe(1);
    expect(citation).toContain("from '../native-lane-26/client'");

    const knowledge = fs.readFileSync(
      path.join(root, 'modules/legal/server/src/adapters/legal/knowledge.ts'),
      'utf8',
    );
    expect(knowledge).not.toContain('legalStatePointSql');
    expect(knowledge).toContain("from '../native-lane-26/client'");
  });

  it('retirement:ts-statements is BEFORE=127 -> AFTER=126', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
        const p = path.join(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts'))
          total += (fs.readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(path.join(root, 'modules/legal/server/src'));
    expect(total).toBeLessThan(127);
    expect(total).toBeLessThanOrEqual(126);
  });
});
