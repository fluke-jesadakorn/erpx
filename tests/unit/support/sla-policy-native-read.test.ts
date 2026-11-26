import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { createSlaPolicyReader } from '../../../packages/server-support/src/adapters/support-sla';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';

const config = JSON.stringify({
  privateBaseUrl: 'http://127.0.0.1:3997',
  timeoutMs: 1000,
  holdings: [{ origin: 'https://acme.jesadakorn.com', holdingSlug: 'acme', holdingId: 'h1' }],
});
const actor = { userId: 'u1', companyId: 'company', holdingId: 'h1', holdingSlug: 'acme' } as unknown as WorkContext;
const incoming = () => new Headers({
  host: 'acme.jesadakorn.com',
  cookie: 'session=abc',
  origin: 'https://acme.jesadakorn.com',
  'x-erpx-context-company': 'company',
  'x-erpx-context-assignment': 'a1',
});
const point = {
  id: 'policy-1', company_id: 'company', name: 'Standard support', status: 'draft', version: 1,
  policy_json: { targetMinutes: 60 }, created_by: 'creator', approved_by: null, approved_at: null,
  created_at: 1, updated_at: 2,
};
const item = {
  id: 'policy-1', type: 'support_policy', title: 'Resolution-time policy · Standard support',
  detail: 'Review the proposed working calendar and target before use.', href: '/support/sla',
  status: 'draft', updatedAt: 2, queue: 'todo', canAct: true,
  responsibleQueue: 'Independent Company Support manager', nextAction: 'Review or retire the proposed policy',
  blocker: null, relationships: ['approval'],
};
const json = (data: unknown, status = 200) => new Response(JSON.stringify(status === 200 ? { data } : { error: data }), {
  status, headers: { 'content-type': 'application/json' },
});

describe('Support SLA policy mounted native read transport', () => {
  test('calls the exact point and work routes with only the caller session envelope', async () => {
    const transport = vi.fn(async (_url: URL, _outgoing: Headers, _signal: AbortSignal) => json([item]));
    const reader = createSlaPolicyReader(config, transport);
    transport.mockImplementationOnce(async () => json(point));
    await expect(reader.point(actor, incoming(), 'policy-1')).resolves.toMatchObject({ id: 'policy-1', status: 'draft', version: 1 });
    expect(String(transport.mock.calls[0]![0])).toBe('http://127.0.0.1:3997/api/v1/holdings/acme/support/sla/policies/policy-1');
    expect(transport.mock.calls[0]![1].get('cookie')).toBe('session=abc');
    expect(transport.mock.calls[0]![1].get('x-erpx-context-company')).toBe('company');
    await expect(reader.work(actor, incoming())).resolves.toEqual([item]);
    expect(String(transport.mock.calls[1]![0])).toBe('http://127.0.0.1:3997/api/v1/holdings/acme/support/sla/policies/work');
  });

  test('fails closed on absent config, mismatched target, rejected read and malformed payload', async () => {
    expect(() => createSlaPolicyReader('')).toThrowError(/reader is unavailable/);
    const reader = createSlaPolicyReader(config, async () => json(point));
    await expect(reader.point(actor, new Headers({ host: 'other.test' }), 'policy-1'))
      .rejects.toMatchObject({ status: 409, code: 'NATIVE_SUPPORT_SLA_POLICY_AUTHORITY_INVALID' });
    const rejected = createSlaPolicyReader(config, async () => json({ code: 'SUPPORT_POLICY_NOT_FOUND', message: 'missing' }, 404));
    await expect(rejected.point(actor, incoming(), 'policy-1'))
      .rejects.toMatchObject({ status: 404, code: 'SUPPORT_POLICY_NOT_FOUND' });
    const malformed = createSlaPolicyReader(config, async () => json({ unexpected: true }));
    await expect(malformed.point(actor, incoming(), 'policy-1'))
      .rejects.toMatchObject({ status: 503, code: 'NATIVE_SUPPORT_SLA_POLICY_READ_UNAVAILABLE' });
    await expect(malformed.work(actor, incoming()))
      .rejects.toMatchObject({ status: 503, code: 'NATIVE_SUPPORT_SLA_POLICY_READ_UNAVAILABLE' });
  });

  test('retires the plan-and-execute point/work kinds without touching the retained Support readers', () => {
    const root = resolve(__dirname, '../../../packages/server-support/src');
    const sla = readFileSync(resolve(root, 'adapters/support-sla.ts'), 'utf8');
    const bridge = readFileSync(resolve(root, 'adapters/native-support-read.ts'), 'utf8');
    const application = readFileSync(resolve(root, 'application/support-sla.ts'), 'utf8');
    expect(sla).toContain('ERPX_NATIVE_SUPPORT_READ_CONFIG');
    expect(sla).toContain('/support/sla/policies');
    expect(sla).toContain("'/work'");
    expect(sla).not.toContain('nativeSupportReadAll');
    expect(sla).not.toContain('SUPPORT_RD_SLA_POLICY_POINT');
    expect(sla).not.toContain('SUPPORT_RD_SLA_POLICY_WORK');
    expect(bridge).not.toContain("SUPPORT_RD_SLA_POLICY_POINT = 'support_rd_sla_policy_point'");
    expect(bridge).not.toContain("SUPPORT_RD_SLA_POLICY_WORK = 'support_rd_sla_policy_work'");
    expect(bridge).toContain("SUPPORT_RD_SLA_POLICY_APPROVED = 'support_rd_sla_policy_approved'");
    expect(bridge).toContain("SUPPORT_RD_SLA_POLICY_WORKSPACE = 'support_rd_sla_policy_workspace'");
    expect(bridge).toContain("SUPPORT_RD_TICKET_BY_ID = 'support_rd_ticket_by_id'");
    expect(application).toContain('createSupportSlaWorkspace');
    expect(application).not.toContain('createSupportPolicyWork');
    expect(application).not.toContain('createGetSupportPolicy');
  });
});
