import { createHash } from 'node:crypto';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const state = vi.hoisted(() => ({ fetch: vi.fn(), write: vi.fn() }));

vi.mock('@erpx/identity-access-server/public/native-identity-write', () => ({
  nativeIdentityWrite: state.write,
}));

import {
  nativeApprovalActiveScope,
  nativeApprovalCanOverride,
  nativeApprovalExpenseEvidence,
  nativeLane12Configured,
  nativePeopleTemplateActiveRole,
  nativePeopleTemplateAttributes,
  nativePeopleTemplateGroupReferences,
  type LaneHoldingScope,
} from '../../../modules/identity-access/server/src/adapters/native-lane-12/client';
import { peopleTemplateAttributes } from '../../../modules/identity-access/server/src/adapters/people-template-attributes';
import { createApprovalsSourceRepository } from '../../../modules/identity-access/server/src/adapters/workflow-approvals-facts';

/** Exact dev config: three registered Holdings, no first-holding default. */
const DEV_CONFIG = {
  privateBaseUrl: 'http://127.0.0.1:3198',
  timeoutMs: 10000,
  holdings: [
    { origin: 'https://acme.jesadakorn.com', host: 'acme.jesadakorn.com', holdingId: 'h001', holdingSlug: 'acme' },
    { origin: 'https://northstar.jesadakorn.com', host: 'northstar.jesadakorn.com', holdingId: 'h002', holdingSlug: 'northstar' },
    { origin: 'https://erpx-internal.jesadakorn.com', host: 'erpx-internal.jesadakorn.com', holdingId: 'h003', holdingSlug: 'erpx', companyId: 'platform' },
  ],
};
const CONFIG = JSON.stringify(DEV_CONFIG);
const acme: LaneHoldingScope = { holdingId: 'h001', holdingSlug: 'acme', companyId: 'c1' };
const northstar: LaneHoldingScope = { holdingId: 'h002', holdingSlug: 'northstar', companyId: 'c2' };

const sha = (value: string) => createHash('sha256').update(value).digest('hex');
const reply = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });
const seen = () => {
  const [url, init] = state.fetch.mock.calls[state.fetch.mock.calls.length - 1] as [URL, RequestInit];
  const headers = init.headers as Record<string, string>;
  return { url, headers };
};

beforeEach(() => {
  vi.stubEnv('ERPX_NATIVE_LANE_12_CONFIG', CONFIG);
  vi.stubGlobal('fetch', state.fetch);
  state.fetch.mockReset();
  state.write.mockReset();
});
afterEach(() => {
  vi.unstubAllEnvs();
  vi.unstubAllGlobals();
});

describe('native lane 12 SQL and result oracle', () => {
  const sql = {
    attributes: 'SELECT id,version,definition FROM um_attributes WHERE company_id=$1 ORDER BY id',
    activeRole: "SELECT 1 FROM iam_roles WHERE id=$1 AND company_id IN ('*',$2) AND status='active'",
    groupReferences:
      "SELECT 1 FROM um_groups WHERE company_id=$1 AND NOT (definition->>'archived')::boolean AND EXISTS(SELECT 1 FROM jsonb_array_elements(definition->'rule'->'conditions') x WHERE x->>'field'=$2)",
  };
  it('pins the three byte-identical retired statements', () => {
    expect(sha(sql.attributes)).toBe('fc2fca4285b7a6baf1681610ba50fec70a983b65c922160ea46c2e7637a7e396');
    expect(sha(sql.activeRole)).toBe('dcb70327c0b64b8492d55fc9333fd992697b01c5b792e576a2e54aad0250522d');
    expect(sha(sql.groupReferences)).toBe('5ae2987e3ed8e9d9d67b77ac23f7fabbdb5d6b5499560f7617ad18bf9b1364e2');
  });
  it('pins the {rows,rowCount} result envelope', () => {
    const page = { rows: [{ id: 'custom:a', version: 2, definition: { type: 'text' } }], rowCount: 1 };
    expect(sha(JSON.stringify(page))).toBe('1147615a15ffaad36f49a02324f43079f3473d8c16f45a73d411d2dfb7c5e7e7');
  });
});

describe('native lane 12 multi-holding transport', () => {
  it('selects the exact registered holding by holdingId + holdingSlug', async () => {
    state.fetch.mockResolvedValueOnce(reply({ rows: [{ id: 'custom:a', version: 2, definition: { type: 'text' } }], rowCount: 1 }));
    state.fetch.mockResolvedValueOnce(reply({ rows: [{ one: 1 }], rowCount: 1 }));
    state.fetch.mockResolvedValueOnce(reply({ rows: [], rowCount: 0 }));
    const page = await nativePeopleTemplateAttributes('c1', acme);
    expect(page.rowCount).toBe(1);
    let request = seen();
    expect(request.url.pathname).toBe('/api/v1/holdings/acme/identity-access/people-template-attributes/attributes');
    expect(request.headers.host).toBe('acme.jesadakorn.com');
    expect(request.headers.origin).toBe('https://acme.jesadakorn.com');
    expect(request.headers['x-erpx-context-company']).toBe('c1');

    await nativePeopleTemplateActiveRole('c2', 'iam.roles.manage', northstar);
    request = seen();
    expect(request.url.pathname).toBe('/api/v1/holdings/northstar/identity-access/people-template-attributes/activeRole');
    expect(request.headers.host).toBe('northstar.jesadakorn.com');
    expect(request.headers['x-erpx-context-company']).toBe('c2');
    expect(request.url.searchParams.get('role')).toBe('iam.roles.manage');

    await nativePeopleTemplateGroupReferences('platform', 'custom:a');
    request = seen();
    expect(request.url.pathname).toBe('/api/v1/holdings/erpx/identity-access/people-template-attributes/groupReferences');
    expect(request.headers.host).toBe('erpx-internal.jesadakorn.com');
    expect(request.headers['x-erpx-context-company']).toBe('platform');
    expect(request.url.searchParams.get('value')).toBe('custom:a');
  });

  it('pins probe rowCount and carries no SQL on the wire', async () => {
    state.fetch.mockResolvedValueOnce(reply({ rows: [{ one: 1 }], rowCount: 1 }));
    expect((await nativePeopleTemplateActiveRole('c1', 'r', acme)).rowCount).toBe(1);
    const [url, init] = state.fetch.mock.calls[0] as [URL, RequestInit];
    expect(url.toString()).not.toMatch(/SELECT/i);
    expect(JSON.stringify(init)).not.toMatch(/SELECT/i);
  });

  it('delegates the retained adapter with the caller scope and no TS SQL', async () => {
    state.fetch.mockImplementation(() => Promise.resolve(reply({ rows: [], rowCount: 0 })));
    const store = peopleTemplateAttributes({} as never, northstar);
    expect((await store.attributes('c2')).rowCount).toBe(0);
    expect((await store.activeRole('role', 'c2')).rowCount).toBe(0);
    expect((await store.groupReferences('c2', 'value')).rowCount).toBe(0);
    expect(state.fetch).toHaveBeenCalledTimes(3);
    for (const [url] of state.fetch.mock.calls as Array<[URL, RequestInit]>)
      expect(url.pathname).toContain('/holdings/northstar/');
    expect(state.write).not.toHaveBeenCalled();
  });
});

describe('native lane 12 fails closed', () => {
  it('rejects unknown/mismatched targets with no first-holding default', async () => {
    for (const scope of [
      { holdingId: 'h999', holdingSlug: 'acme', companyId: 'c1' },
      { holdingId: 'h001', holdingSlug: 'northstar', companyId: 'c1' },
      { holdingId: 'h002', holdingSlug: 'acme', companyId: 'c1' },
    ])
      await expect(nativePeopleTemplateAttributes('c1', scope)).rejects.toMatchObject({
        status: 503,
        code: 'NATIVE_LANE_12_READ_TARGET_UNAVAILABLE',
      });
    expect(state.fetch).not.toHaveBeenCalled();

    vi.stubEnv('ERPX_NATIVE_LANE_12_CONFIG', JSON.stringify({ ...DEV_CONFIG, holdings: DEV_CONFIG.holdings.slice(0, 2) }));
    await expect(nativePeopleTemplateAttributes('c1')).rejects.toMatchObject({
      status: 503,
      code: 'NATIVE_LANE_12_READ_TARGET_UNAVAILABLE',
    });
  });

  it('validates 1..64 holdings and rejects malformed configuration', async () => {
    for (const holdings of [[], Array.from({ length: 65 }, (_, i) => ({
      origin: `https://h${i}.jesadakorn.com`,
      host: `h${i}.jesadakorn.com`,
      holdingId: `h${i}`,
      holdingSlug: `h${i}`,
    }))]) {
      vi.stubEnv('ERPX_NATIVE_LANE_12_CONFIG', JSON.stringify({ ...DEV_CONFIG, holdings }));
      await expect(nativePeopleTemplateAttributes('c1', acme)).rejects.toMatchObject({
        status: 503,
        code: 'NATIVE_LANE_12_READ_CONFIGURATION_INVALID',
      });
    }
    vi.stubEnv('ERPX_NATIVE_LANE_12_CONFIG', undefined);
    expect(nativeLane12Configured()).toBe(false);
    await expect(nativePeopleTemplateAttributes('c1', acme)).rejects.toMatchObject({
      status: 503,
      code: 'NATIVE_LANE_12_READ_CONFIGURATION_REQUIRED',
    });
    expect(state.fetch).not.toHaveBeenCalled();
  });

  it('rejects transport failure, 5xx and malformed responses with 503', async () => {
    state.fetch.mockRejectedValueOnce(new Error('ECONNREFUSED'));
    await expect(nativePeopleTemplateAttributes('c1', acme)).rejects.toMatchObject({ status: 503 });

    state.fetch.mockResolvedValueOnce(reply({ error: { code: 'X', message: 'down' } }, 500));
    await expect(nativePeopleTemplateAttributes('c1', acme)).rejects.toMatchObject({ status: 503 });

    state.fetch.mockResolvedValueOnce(reply({ rows: [{ id: 'a' }], rowCount: 1 }));
    await expect(nativePeopleTemplateAttributes('c1', acme)).rejects.toMatchObject({
      status: 503,
      code: 'NATIVE_LANE_12_READ_RESPONSE_INVALID',
    });

    state.fetch.mockResolvedValueOnce(reply({ rows: [], rowCount: 7 }));
    await expect(nativePeopleTemplateAttributes('c1', acme)).rejects.toMatchObject({ status: 503 });

    state.fetch.mockResolvedValueOnce(reply({ rows: [{ one: 2 }], rowCount: 1 }));
    await expect(nativePeopleTemplateActiveRole('c1', 'r', acme)).rejects.toMatchObject({ status: 503 });
  });

  it('propagates an owner 4xx rejection and rejects an out-of-scope company', async () => {
    state.fetch.mockResolvedValueOnce(reply({ error: { code: 'NATIVE_LANE_12_READ_REQUEST_INVALID', message: 'bad' } }, 422));
    await expect(nativePeopleTemplateAttributes('c1', acme)).rejects.toMatchObject({
      status: 422,
      code: 'NATIVE_LANE_12_READ_REQUEST_INVALID',
    });
    await expect(nativePeopleTemplateAttributes('evil\ncompany', acme)).rejects.toMatchObject({
      status: 422,
      code: 'NATIVE_LANE_12_READ_REQUEST_INVALID',
    });
    expect(state.fetch).toHaveBeenCalledTimes(1);
  });
});

describe('native lane 12 workflow approval read family', () => {
  const sql = {
    override:
      "SELECT 1 FROM assignments WHERE id=? AND company_id=? AND user_id=? AND iam_role_allows(role,company_id,'workflow.people_manager') AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND (scope_kind='holding' OR scope_kind='branch' AND scope_node_id=? OR scope_kind='department' AND scope_node_id=?)",
    activeScope:
      'SELECT 1 FROM assignments WHERE id=? AND user_id=? AND company_id=? AND role=? AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>? AND (? OR scope_kind=\'holding\' OR scope_kind=\'branch\' AND scope_node_id=? OR scope_kind=\'department\' AND scope_node_id=?)',
    expenseEvidence:
      'SELECT id,role,scope_kind,scope_node_id,valid_from,valid_until FROM assignments WHERE company_id=? AND id=? AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>? ORDER BY id',
  };

  it('pins the three byte-identical retired approval statements', () => {
    expect(sha(sql.override)).toBe('a269b4e5d9157b2eb2660ebd79cdc9d9c5aa678ef87e2777374cdd23628dbe70');
    expect(sha(sql.activeScope)).toBe('8aee2c29092d5726fb0e0b4d1465e27d78ee766177b310fb0c7dbb7de2ac7dd9');
    expect(sha(sql.expenseEvidence)).toBe('bf5b9b5ba190c9fa422fb45a9ca904484ec1d408e726db2d16673ca8f58c8683');
  });

  it('maps first-row reads to row-or-null and .all() reads to {results}', async () => {
    state.fetch.mockResolvedValueOnce(reply({ rows: [{ one: 1 }], rowCount: 1 }));
    expect(await nativeApprovalCanOverride('c1', 'a1', 'u1', 'b1', null, acme)).toEqual({ one: 1 });
    let request = seen();
    expect(request.url.pathname).toBe('/api/v1/holdings/acme/identity-access/approval-facts/canOverrideStaffStepRole');
    expect(request.url.searchParams.get('assignment')).toBe('a1');
    expect(request.url.searchParams.get('branch')).toBe('b1');
    expect(request.url.searchParams.has('department')).toBe(false);
    expect(request.headers['x-erpx-context-company']).toBe('c1');

    state.fetch.mockResolvedValueOnce(reply({ rows: [], rowCount: 0 }));
    expect(await nativeApprovalActiveScope('c1', 'a1', 'u1', 'r', 1, 2, false, null, null, northstar)).toBeNull();
    request = seen();
    expect(request.url.searchParams.get('policyStep')).toBe('false');
    expect(request.url.pathname).toContain('/holdings/northstar/');

    const rows = [{ id: 'a1', role: 'workflow.approve', scope_kind: 'holding', scope_node_id: null, valid_from: 1, valid_until: 2 }];
    state.fetch.mockResolvedValueOnce(reply({ rows, rowCount: 1 }));
    expect(await nativeApprovalExpenseEvidence('c1', 'a1', 1, 2, acme)).toEqual({ results: rows });
    expect(seen().url.pathname).toContain('/approval-facts/expenseDecisionEvidence');
  });

  it('keeps the retained repository as a transport with no SQL fallback', async () => {
    state.fetch.mockImplementation(() => Promise.resolve(reply({ rows: [], rowCount: 0 })));
    const store = createApprovalsSourceRepository({} as never, northstar);
    expect(await store.readCanOverrideStaffStepRole('a1', 'c2', 'u1', null, null)).toBeNull();
    expect(await store.readApprovalViewActiveScope('a1', 'u1', 'c2', 'r', 1, 2, false, null, null)).toBeNull();
    expect(await store.readExpenseDecisionEvidenceAssignmentsreceiptsperson('c2', 'a1', 1, 2)).toEqual({ results: [] });
    expect(state.fetch).toHaveBeenCalledTimes(3);
    for (const [url, init] of state.fetch.mock.calls as Array<[URL, RequestInit]>) {
      expect(url.pathname).toContain('/holdings/northstar/identity-access/approval-facts/');
      expect(JSON.stringify(init)).not.toMatch(/SELECT/i);
    }
  });

  it('fails closed on malformed approval responses and unknown targets', async () => {
    state.fetch.mockResolvedValueOnce(reply({ rows: [{ id: 'a1' }], rowCount: 1 }));
    await expect(nativeApprovalExpenseEvidence('c1', 'a1', 1, 2, acme)).rejects.toMatchObject({
      status: 503,
      code: 'NATIVE_LANE_12_READ_RESPONSE_INVALID',
    });
    await expect(
      nativeApprovalCanOverride('c1', 'a1', 'u1', null, null, { holdingId: 'h999', holdingSlug: 'acme', companyId: 'c1' }),
    ).rejects.toMatchObject({ status: 503, code: 'NATIVE_LANE_12_READ_TARGET_UNAVAILABLE' });
  });
});
