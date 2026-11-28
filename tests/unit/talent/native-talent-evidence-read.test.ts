import { describe, expect, it, vi, afterEach } from 'vitest';
vi.mock('server-only', () => ({}));
vi.mock('@erpx/platform-runtime-ui/adapters/server/native-executive-evidence', () => ({
  recordExecutiveEvidenceAccess: vi.fn(async () => ({ success: true })),
}));
import { nativeTalentEvidenceRow } from '../../../platform/runtime/server/src/adapters/executive/talent-evidence';

const ENV = 'ERPX_NATIVE_TALENT_EVIDENCE_CONFIG';
const config = JSON.stringify({
  privateBaseUrl: 'http://127.0.0.1:44994',
  timeoutMs: 1000,
  holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' }],
});

const actor = {
  holdingId: 'h001',
  holdingSlug: 'acme',
  companyId: 'company-1',
  userId: 'user-1',
  branchId: 'branch-1',
  departmentId: null,
  authority: { revision: 7, assignmentVersion: 3 },
  iam: { permissions: ['records.read', 'records.review'] },
} as never;

const row = {
  id: 'req-1',
  job_title: 'Engineer',
  target_start_date: '2026-12-01',
  status: 'open',
  version: 2,
  updated_at: 1_800_000_000_000,
  branch_id: 'branch-1',
  department_id: null,
};
const ok = (data: unknown) => new Response(JSON.stringify({ data }), {
  status: 200, headers: { 'content-type': 'application/json' },
});

afterEach(() => {
  vi.unstubAllGlobals();
  if (process.env[ENV] === undefined) delete process.env[ENV];
});

describe('runtime talent executive-evidence strict native point read', () => {
  it('reads the full owner row from the mounted native route with scope headers', async () => {
    process.env[ENV] = config;
    const fetchMock = vi.fn(async (_input: RequestInfo | URL, _init?: RequestInit) => ok(row));
    vi.stubGlobal('fetch', fetchMock);
    const value = await nativeTalentEvidenceRow(
      actor, 'open-requisitions', 'req-1',
    );
    expect(value).toEqual(row);
    const url = String(fetchMock.mock.calls[0][0]);
    expect(url).toContain('/api/v1/holdings/acme/talent/executive-evidence/open-requisitions/req-1');
    const init = fetchMock.mock.calls[0][1] as { headers: Headers };
    expect(init.headers.get('x-erpx-context-company')).toBe('company-1');
    expect(init.headers.get('x-erpx-context-actor')).toBe('user-1');
    expect(init.headers.get('x-erpx-context-branch')).toBe('branch-1');
    expect(init.headers.has('x-erpx-context-department')).toBe(false);
    expect(init.headers.has('x-erpx-evidence-from')).toBe(false);
  });

  it('forwards the application update window and keeps a missing row null', async () => {
    process.env[ENV] = config;
    const fetchMock = vi.fn(async (_input: RequestInfo | URL, _init?: RequestInit) => ok(null));
    vi.stubGlobal('fetch', fetchMock);
    const value = await nativeTalentEvidenceRow(
      actor, 'application-stage-counts', 'app-missing', { from: 10, toExclusive: 20 },
    );
    expect(value).toBeNull();
    const init = fetchMock.mock.calls[0][1] as { headers: Headers };
    expect(init.headers.get('x-erpx-evidence-from')).toBe('10');
    expect(init.headers.get('x-erpx-evidence-to')).toBe('20');
  });

  it('fails closed with no configuration and never calls fetch', async () => {
    delete process.env[ENV];
    const fetchMock = vi.fn(async () => ok(row));
    vi.stubGlobal('fetch', fetchMock);
    await expect(nativeTalentEvidenceRow(actor, 'open-requisitions', 'req-1'))
      .rejects.toMatchObject({ code: 'NATIVE_TALENT_EVIDENCE_CONFIGURATION_REQUIRED', status: 503 });
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('fails closed on a rejected native reply and on a malformed row', async () => {
    process.env[ENV] = config;
    vi.stubGlobal('fetch', vi.fn(async () => new Response(
      JSON.stringify({ error: { code: 'EVIDENCE_NOT_FOUND' } }), { status: 404 },
    )));
    await expect(nativeTalentEvidenceRow(actor, 'open-requisitions', 'req-1'))
      .rejects.toMatchObject({ code: 'EVIDENCE_NOT_FOUND', status: 404 });

    vi.stubGlobal('fetch', vi.fn(async () => ok({ id: 'req-1', unexpected: true })));
    await expect(nativeTalentEvidenceRow(actor, 'open-requisitions', 'req-1'))
      .rejects.toMatchObject({ code: 'NATIVE_TALENT_EVIDENCE_RESPONSE_INVALID', status: 502 });
  });
});
