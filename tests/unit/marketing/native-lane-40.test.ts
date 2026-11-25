import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { createHash } from 'node:crypto';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { ApiError } from '@erpx/server-runtime/public/http';
import {
  LANE40_ACTIVE_FORM_SOURCE,
  LANE40_CAMPAIGN_SUMMARY,
  LANE40_CANDIDATE_DETAIL,
  LANE40_CANDIDATE_HISTORY,
  LANE40_CANDIDATE_LIST,
  LANE40_FORM_CONTENT_ROW,
  LANE40_PUBLIC_RECEIPT,
  nativeLane40Configured,
  readMarketingLane40,
  readMarketingLane40ForCompany,
} from '../../../packages/server-marketing/src/adapters/native-lane-40/client';

vi.mock('server-only', () => ({}));

const ENV = 'ERPX_NATIVE_MARKETING_LANE40_CONFIG';
const original = process.env[ENV];
const sha = (value: string) => createHash('sha256').update(value).digest('hex');
const ok = (data: unknown) => new Response(JSON.stringify({ data }), {
  status: 200,
  headers: { 'content-type': 'application/json' },
});
const actor = { holdingId: 'h001', holdingSlug: 'acme', companyId: 'company-1' };
const holding = (id: string, slug: string, domain: string) => ({
  origin: `https://${domain}`,
  host: domain,
  holdingId: id,
  holdingSlug: slug,
  companyId: `company-${slug}`,
});

// Rendered SQL oracles. The Rust lane owns these exact statements; the bridge
// never emits them. Hashes pin the byte-level query contract across the cutover.
const GUARD = "EXISTS(SELECT 1 FROM (SELECT candidate.* FROM marketing_lead_candidates candidate JOIN marketing_assets form ON form.id=candidate.form_id AND form.company_id=candidate.company_id AND form.kind='form' AND form.version>=candidate.form_version JOIN marketing_asset_events captured ON captured.asset_id=form.id AND captured.version=candidate.form_version AND captured.action='activate' AND captured.status='active' JOIN marketing_engagement_events submission ON submission.candidate_id=candidate.id AND submission.version=1 AND submission.event_type='form_submit' WHERE length(trim(candidate.consent_evidence))>=3) native_candidate WHERE native_candidate.id=c.id AND native_candidate.company_id=c.company_id)";
const PROJ = "(to_jsonb(c) || jsonb_build_object('form_name',a.name))::text AS value";
const FROM = " FROM marketing_lead_candidates c JOIN marketing_assets a ON a.id=c.form_id AND a.company_id=c.company_id";
const SCOPE = " AND ($2::text IS NULL OR c.branch_id=$2) AND ($3::text IS NULL OR c.department_id=$3)";
const SQL_ORACLES: Record<string, { hash: string; length: number; tables: string[] }> = {
  [LANE40_CANDIDATE_LIST]: {
    hash: 'f05818509ff062cfcac513ce1b07fff1d8527fb1011a0a9686012173b76619d6',
    length: 1026,
    tables: ['marketing_lead_candidates', 'marketing_assets', 'marketing_asset_events', 'marketing_engagement_events'],
  },
  [LANE40_CANDIDATE_DETAIL]: {
    hash: 'fc1082e92dd273d6466dcd39ba66637e48ebd048e6eb8a594995f75dab304a2e',
    length: 302,
    tables: ['marketing_lead_candidates', 'marketing_assets'],
  },
  [LANE40_CANDIDATE_HISTORY]: {
    hash: '446ce35fcc1512f6835d24722d3d0fafaf3166d810e1f36b9090917a799922b8',
    length: 238,
    tables: ['marketing_engagement_events', 'marketing_lead_candidates'],
  },
  [LANE40_CAMPAIGN_SUMMARY]: {
    hash: '452695860be0a26c81ffbd6f4a8fbf3a4546418647c9050d8a8a572cf5c1b440',
    length: 1373,
    tables: ['marketing_lead_candidates', 'marketing_assets', 'marketing_asset_events', 'marketing_engagement_events'],
  },
};
const renderedSql: Record<string, string> = {
  [LANE40_CANDIDATE_LIST]: `SELECT ${PROJ}${FROM} WHERE c.company_id=$1 AND ${GUARD}${SCOPE} ORDER BY c.updated_at DESC`,
  [LANE40_CANDIDATE_DETAIL]: `SELECT ${PROJ}${FROM} WHERE c.id=$4 AND c.company_id=$1${SCOPE}`,
  [LANE40_CANDIDATE_HISTORY]: "SELECT (to_jsonb(e) || jsonb_build_object('company_id',c.company_id))::text AS value FROM marketing_engagement_events e JOIN marketing_lead_candidates c ON c.id=e.candidate_id AND c.company_id=$2 WHERE e.candidate_id=$1 ORDER BY e.version",
  [LANE40_CAMPAIGN_SUMMARY]: `SELECT COALESCE(NULLIF(BTRIM(c.attribution_json::jsonb->>'campaign'),''),'Unspecified') campaign,COALESCE(NULLIF(BTRIM(c.attribution_json::jsonb->>'source'),''),NULLIF(BTRIM(c.attribution_json::jsonb->>'utmSource'),''),'Unspecified') source,COUNT(*)::bigint captured,COUNT(*) FILTER (WHERE c.status='pending')::bigint pending,COUNT(*) FILTER (WHERE c.status='handed_off')::bigint handed_off,COUNT(*) FILTER (WHERE c.status='rejected')::bigint rejected,$1::text company_id FROM marketing_lead_candidates c WHERE c.company_id=$1 AND ${GUARD}${SCOPE} GROUP BY 1,2 ORDER BY COUNT(*) DESC,campaign,source`,
};

const candidateRow = { id: 'marketing-candidate-1', company_id: 'company-1', form_name: 'Lead form', status: 'pending' };
const summaryRow = { company_id: 'company-1', campaign: 'Unspecified', source: 'Unspecified', captured: 4, pending: 1, handed_off: 2, rejected: 1 };
const historyRow = { id: 'engagement-1', company_id: 'company-1', event_type: 'form_submit' };
const rowsFor = (kind: string) =>
  kind === LANE40_CAMPAIGN_SUMMARY ? [summaryRow] : kind === LANE40_CANDIDATE_HISTORY ? [historyRow] : [candidateRow];

describe('native Marketing lane-40 read client', () => {
  beforeEach(() => {
    process.env[ENV] = JSON.stringify({
      privateBaseUrl: 'http://127.0.0.1:44995',
      timeoutMs: 1000,
      holdings: [holding('h001', 'acme', 'acme.example.com')],
    });
  });
  afterEach(() => {
    if (original === undefined) delete process.env[ENV];
    else process.env[ENV] = original;
    vi.unstubAllGlobals();
  });

  it('posts one closed kind to the exact Holding route and pins the result oracle', async () => {
    for (const kind of Object.keys(SQL_ORACLES)) {
      let url = '';
      let init: RequestInit | undefined;
      vi.stubGlobal('fetch', vi.fn(async (u: URL, i: RequestInit) => {
        url = String(u);
        init = i;
        return ok({ kind, companyId: actor.companyId, rows: rowsFor(kind) });
      }));
      const rows = await readMarketingLane40(kind, actor, {
        branchId: null,
        departmentId: null,
        recordId: 'marketing-candidate-1',
      });
      expect(rows).toEqual(rowsFor(kind));
      expect(url).toBe('http://127.0.0.1:44995/api/v1/holdings/acme/marketing/lane-40/read');
      expect(init?.method).toBe('POST');
      const headers = init?.headers as Record<string, string>;
      expect(headers['host']).toBe('acme.example.com');
      expect(headers['origin']).toBe('https://acme.example.com');
      expect(headers['x-erpx-context-company']).toBe('company-1');
      expect(JSON.parse(String(init?.body))).toEqual({
        kind,
        branchId: null,
        departmentId: null,
        recordId: 'marketing-candidate-1',
        assetId: null,
        version: null,
        enforced: false,
      });
    }
    expect(sha(JSON.stringify(summaryRow))).toBe('c9fc2d79260454b50ce986211f9b304e41fa99190c511f35b512b94a953d777a');
  });

  it('routes the three wave-2 owner reads with closed scope and validated rows', async () => {
    const cases = [
      {
        kind: LANE40_ACTIVE_FORM_SOURCE,
        scope: { assetId: 'form-1', version: 3 },
        row: { company_id: 'company-acme', configuration_json: '{"f":"v"}' },
      },
      {
        kind: LANE40_FORM_CONTENT_ROW,
        scope: { assetId: 'content-1', version: 2 },
        row: { company_id: 'company-acme', id: 'content-1', version: 2, preview_json: '{"p":"c"}' },
      },
      {
        kind: LANE40_PUBLIC_RECEIPT,
        scope: { recordId: 'marketing-candidate-1' },
        row: { company_id: 'company-acme', request_hash: 'a'.repeat(64), candidate_id: 'marketing-candidate-1' },
      },
    ];
    for (const current of cases) {
      let sent: Record<string, unknown> | undefined;
      vi.stubGlobal('fetch', vi.fn(async (_url: URL, init: RequestInit) => {
        sent = JSON.parse(String(init.body)) as Record<string, unknown>;
        return ok({ kind: current.kind, companyId: 'company-acme', rows: [current.row] });
      }));
      const rows = await readMarketingLane40ForCompany(current.kind, 'company-acme', current.scope);
      expect(rows).toEqual([current.row]);
      expect(sent).toMatchObject({ kind: current.kind, ...current.scope });
    }
    const noFetch = vi.fn(async () => ok({ data: {} }));
    vi.stubGlobal('fetch', noFetch);
    await expect(readMarketingLane40ForCompany(LANE40_ACTIVE_FORM_SOURCE, 'company-acme', { assetId: 'form-1' }))
      .rejects.toMatchObject({ code: 'MARKETING_LANE40_READ_REQUEST_INVALID', status: 503 });
    await expect(readMarketingLane40ForCompany(LANE40_FORM_CONTENT_ROW, 'company-acme', { assetId: 'content-1', version: 0 }))
      .rejects.toMatchObject({ code: 'MARKETING_LANE40_READ_REQUEST_INVALID', status: 503 });
    await expect(readMarketingLane40ForCompany(LANE40_PUBLIC_RECEIPT, 'company-acme', {}))
      .rejects.toMatchObject({ code: 'MARKETING_LANE40_READ_REQUEST_INVALID', status: 503 });
    expect(noFetch).not.toHaveBeenCalled();
    vi.stubGlobal('fetch', vi.fn(async () => ok({
      kind: LANE40_PUBLIC_RECEIPT,
      companyId: 'company-acme',
      rows: [{ ...cases[2]!.row, company_id: 'company-other' }],
    })));
    await expect(readMarketingLane40ForCompany(LANE40_PUBLIC_RECEIPT, 'company-acme', { recordId: 'marketing-candidate-1' }))
      .rejects.toMatchObject({ status: 503 });
  });

  it('pins every rendered SQL oracle and its owner-local table set', async () => {
    const rust = await readFile(new URL('../../../packages/server-marketing/src/adapters/native_lane_40/postgres.rs', import.meta.url), 'utf8');
    const statements = rust.split('#[cfg(test)]')[0]!;
    expect(statements).toContain('sqlx::query');
    expect(statements).not.toContain('crm_leads');
    expect(statements).not.toContain('command_receipts');
    for (const [kind, sql] of Object.entries(renderedSql)) {
      const oracle = SQL_ORACLES[kind]!;
      expect(sha(sql), kind).toBe(oracle.hash);
      expect(sql.length, kind).toBe(oracle.length);
      for (const table of oracle.tables) expect(sql, `${kind}/${table}`).toContain(table);
      expect(sql).not.toMatch(/FOR (UPDATE|SHARE)|LOCK TABLE/);
    }
    const client = await readFile(new URL('../../../packages/server-marketing/src/adapters/native-lane-40/client.ts', import.meta.url), 'utf8');
    expect(client).not.toMatch(/SELECT /);
    expect(client).toContain('MARKETING_LANE40_READ_UNAVAILABLE');
  });

  it('fails closed on cross-company rows, broken aggregates and wrong envelopes', async () => {
    const badRows = [
      { ...candidateRow, company_id: 'company-2' },
      { ...summaryRow, pending: 2 },
      { ...summaryRow, captured: '4' },
      { ...candidateRow, id: 'bad; DROP TABLE' },
      { ...candidateRow, extra: {} },
    ];
    for (const row of badRows) {
      vi.stubGlobal('fetch', vi.fn(async () => ok({ kind: LANE40_CANDIDATE_LIST, companyId: 'company-1', rows: [row] })));
      await expect(readMarketingLane40(LANE40_CANDIDATE_LIST, actor)).rejects.toMatchObject({
        code: 'MARKETING_LANE40_READ_UNAVAILABLE',
        status: 503,
      });
    }
    vi.stubGlobal('fetch', vi.fn(async () => ok({ kind: LANE40_CAMPAIGN_SUMMARY, companyId: 'company-2', rows: [summaryRow] })));
    await expect(readMarketingLane40(LANE40_CAMPAIGN_SUMMARY, actor)).rejects.toMatchObject({ status: 503 });
    vi.stubGlobal('fetch', vi.fn(async () => ok({ kind: LANE40_CAMPAIGN_SUMMARY, companyId: 'company-1', rows: 'nope' })));
    await expect(readMarketingLane40(LANE40_CAMPAIGN_SUMMARY, actor)).rejects.toBeInstanceOf(ApiError);
  });

  it('fails closed without config, on fetch failure, 5xx and malformed JSON', async () => {
    delete process.env[ENV];
    expect(nativeLane40Configured()).toBe(false);
    vi.stubGlobal('fetch', vi.fn(async () => { throw new Error('must not fetch'); }));
    await expect(readMarketingLane40(LANE40_CANDIDATE_LIST, actor)).rejects.toMatchObject({
      code: 'MARKETING_LANE40_CONFIGURATION_REQUIRED',
      status: 503,
    });
    process.env[ENV] = JSON.stringify({
      privateBaseUrl: 'http://127.0.0.1:44995', timeoutMs: 1000,
      holdings: [holding('h001', 'acme', 'acme.example.com')],
    });
    vi.stubGlobal('fetch', vi.fn(async () => { throw new Error('offline'); }));
    await expect(readMarketingLane40(LANE40_CANDIDATE_LIST, actor)).rejects.toMatchObject({
      code: 'MARKETING_LANE40_READ_UNAVAILABLE', status: 503,
    });
    vi.stubGlobal('fetch', vi.fn(async () => new Response('{}', { status: 503 })));
    await expect(readMarketingLane40(LANE40_CANDIDATE_DETAIL, actor, { recordId: 'marketing-candidate-1' }))
      .rejects.toMatchObject({ status: 503 });
    vi.stubGlobal('fetch', vi.fn(async () => new Response('not json', { status: 200 })));
    await expect(readMarketingLane40(LANE40_CANDIDATE_LIST, actor)).rejects.toMatchObject({ status: 503 });
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({
      error: { code: 'ORIGIN_FORBIDDEN', message: 'Use the registered application origin.' },
    }), { status: 403 })));
    await expect(readMarketingLane40(LANE40_CANDIDATE_LIST, actor)).rejects.toMatchObject({
      code: 'ORIGIN_FORBIDDEN', status: 403,
    });
  });

  it('rejects unregistered holding and invalid scope before any fetch', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => { throw new Error('must not fetch'); }));
    await expect(readMarketingLane40(LANE40_CANDIDATE_LIST, { ...actor, holdingId: 'h999' }))
      .rejects.toMatchObject({ code: 'WORK_CONTEXT_CHANGED', status: 409 });
    await expect(readMarketingLane40('bogus.read', actor))
      .rejects.toMatchObject({ code: 'MARKETING_LANE40_READ_REQUEST_INVALID' });
    await expect(readMarketingLane40(LANE40_CANDIDATE_HISTORY, actor))
      .rejects.toMatchObject({ code: 'MARKETING_LANE40_READ_REQUEST_INVALID' });
    await expect(readMarketingLane40(LANE40_CANDIDATE_LIST, actor, { limit: 5001 }))
      .rejects.toMatchObject({ code: 'MARKETING_LANE40_READ_REQUEST_INVALID' });
  });

  it('selects a Holding only by exact WorkContext id+slug, never by first position', async () => {
    process.env[ENV] = JSON.stringify({
      privateBaseUrl: 'http://127.0.0.1:44995',
      timeoutMs: 1000,
      holdings: [
        holding('h001', 'acme', 'acme.example.com'),
        holding('h002', 'northstar', 'northstar.example.com'),
      ],
    });
    const calls: string[] = [];
    vi.stubGlobal('fetch', vi.fn(async (u: URL) => {
      calls.push(String(u));
      return ok({ kind: LANE40_CANDIDATE_LIST, companyId: 'company-2', rows: [] });
    }));
    await readMarketingLane40(LANE40_CANDIDATE_LIST, {
      holdingId: 'h002', holdingSlug: 'northstar', companyId: 'company-2',
    });
    expect(calls).toEqual(['http://127.0.0.1:44995/api/v1/holdings/northstar/marketing/lane-40/read']);
    // Matching only one dimension must not fall through to another Holding.
    for (const partial of [
      { holdingId: 'h001', holdingSlug: 'northstar' },
      { holdingId: 'h999', holdingSlug: 'northstar' },
      { holdingId: 'h002', holdingSlug: 'acme' },
    ]) {
      await expect(readMarketingLane40(LANE40_CANDIDATE_LIST, {
        ...partial, companyId: 'company-2',
      })).rejects.toMatchObject({ code: 'WORK_CONTEXT_CHANGED', status: 409 });
    }
    expect(calls).toHaveLength(1);
  });

  it('accepts 1..64 standardized holdings and rejects malformed or duplicate entries', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => { throw new Error('must not fetch'); }));
    const config = (holdings: unknown[]) => JSON.stringify({
      privateBaseUrl: 'http://127.0.0.1:44995', timeoutMs: 1000, holdings,
    });
    expect(nativeLane40Configured({ [ENV]: config([holding('h001', 'acme', 'acme.example.com')]) })).toBe(true);
    const { companyId: _optional, ...withoutCompany } = holding('h001', 'acme', 'acme.example.com');
    process.env[ENV] = config([withoutCompany]);
    await expect(readMarketingLane40(LANE40_CANDIDATE_LIST, actor))
      .rejects.toMatchObject({ status: 503 }); // accepted config; selected; fetch still throws
    const many = Array.from({ length: 64 }, (_, index) =>
      holding(`h${index + 1}`, `slug${index + 1}`, `host${index + 1}.example.com`));
    process.env[ENV] = config(many);
    await expect(readMarketingLane40(LANE40_CANDIDATE_LIST, {
      holdingId: 'h64', holdingSlug: 'slug64', companyId: 'company-1',
    })).rejects.toMatchObject({ status: 503 }); // selected, but fetch is stubbed to throw
    for (const bad of [
      [],
      [...many, holding('h65', 'slug65', 'host65.example.com')],
      [holding('h001', 'acme', 'acme.example.com'), holding('h001', 'acme', 'acme.example.com')],
      [{ ...holding('h001', 'acme', 'acme.example.com'), host: 'wrong.example.com' }],
      [{ origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' }],
    ]) {
      process.env[ENV] = config(bad);
      await expect(readMarketingLane40(LANE40_CANDIDATE_LIST, actor))
        .rejects.toMatchObject({ code: 'MARKETING_LANE40_CONFIG_INVALID', status: 503 });
    }
  });

  it('retirement:ts-statements KPI is 9 for the owner (BEFORE=17; wave 2 retired 1)', async () => {
    const root = resolve('packages/server-marketing/src');
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(path);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(path, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(root);
    expect(total).toBe(9);
    const workspace = await readFile(new URL('../../../packages/server-marketing/src/adapters/marketing.ts', import.meta.url), 'utf8');
    expect(workspace).not.toMatch(/SELECT COALESCE\(NULLIF\(BTRIM\(attribution_json/);
    expect(workspace).toContain('readMarketingLane40');
    await expect(readFile(new URL('../../../packages/server-marketing/src/adapters/candidate-form-rows.ts', import.meta.url), 'utf8')).rejects.toThrow();
  });
});
