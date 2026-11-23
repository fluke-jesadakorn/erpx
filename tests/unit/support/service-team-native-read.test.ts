import { readFile } from 'node:fs/promises';
import { describe, expect, it, vi } from 'vitest';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';

vi.mock('server-only', () => ({}));

const source = (path: string) => readFile(new URL(`../../../${path}`, import.meta.url), 'utf8');
const config = JSON.stringify({
  privateBaseUrl: 'http://127.0.0.1:3997',
  timeoutMs: 1000,
  holdings: [{ origin: 'https://acme.jesadakorn.com', holdingSlug: 'acme', holdingId: 'h1', companyId: 'company' }],
});
const ok = (sql: string, values: unknown[]) =>
  new Response(JSON.stringify({ data: { statements: [{ sql, values }] } }), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  });

describe('support service-team coverage native read', () => {
  it('carries no retired owner SQL in the switched adapters', async () => {
    const history = await source('packages/server-support/src/adapters/flow-settings-facts.ts');
    const teams = await source('packages/server-support/src/adapters/service-team-reads.ts');
    const employee = await source('packages/server-support/src/adapters/records-employee-requests-owned-facts.ts');
    const routing = await source('packages/server-support/src/adapters/workflow-flow-routing-facts.ts');
    for (const adapter of [history, employee, routing]) expect(adapter).not.toContain('SELECT');
    expect(teams).not.toContain('service_team_coverage c JOIN');
    expect(teams).not.toContain('service_team_revisions');
  });

  it('plans the service-team name lookup in Rust with owner-scoped binds', async () => {
    const prepare = vi.fn(() => ({ bind: vi.fn() }));
    const batch = vi.fn(async () => [{ results: [{ name: 'Support' }], success: true, meta: { changes: 0, duration: 0 } }]);
    const db = { prepare, batch } as unknown as SqlDatabase;
    process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG = config;
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation(async () =>
      ok('SELECT name FROM service_teams WHERE id=? AND company_id=?', ['team-1', 'company']),
    );
    try {
      const { EmployeeRequestsSupportOwnedFacts } = await import('@erpx/server-support/adapters/records-employee-requests-owned-facts');
      await expect(EmployeeRequestsSupportOwnedFacts(db).readEmployeeRequestDetailTeam('team-1', 'company')).resolves.toEqual({ name: 'Support' });
      expect(JSON.parse(String(fetchMock.mock.calls[0]![1]!.body))).toMatchObject({
        kind: 'support_rd_service_team_name',
        values: ['team-1', 'company'],
      });

      fetchMock.mockClear();
      const { createFlowRoutingSourceRepository } = await import('@erpx/server-support/adapters/workflow-flow-routing-facts');
      await expect(createFlowRoutingSourceRepository(db).readDefinitionsTeam('company', 'team-1')).resolves.toEqual({ name: 'Support' });
      expect(JSON.parse(String(fetchMock.mock.calls[0]![1]!.body))).toMatchObject({
        kind: 'support_rd_service_team_name',
        values: ['team-1', 'company'],
      });
    } finally {
      fetchMock.mockRestore();
      delete process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG;
    }
  });

  it('plans service coverage history in Rust and executes on the caller unit', async () => {
    const prepare = vi.fn(() => ({ bind: vi.fn() }));
    const batch = vi.fn(async () => [{
      results: [{ title: 'Service coverage', version: 2, actor: 'owner', at: 1, kind: 'Service coverage' }],
      success: true,
      meta: { changes: 0, duration: 0 },
    }]);
    const db = { prepare, batch } as unknown as SqlDatabase;
    process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG = config;
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      ok('SELECT t.name title FROM service_team_revisions r', ['company']),
    );
    try {
      const { serviceCoverageSettingsHistory } = await import('@erpx/server-support/adapters/flow-settings-facts');
      await expect(serviceCoverageSettingsHistory(db, 'company')).resolves.toMatchObject([{ version: 2 }]);
      const call = fetchMock.mock.calls[0]!;
      expect(String(call[0])).toContain('/support/command-plan');
      expect(JSON.parse(String(call[1]!.body))).toMatchObject({
        kind: 'support_rd_service_coverage_history',
        values: ['company'],
      });
    } finally {
      fetchMock.mockRestore();
      delete process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG;
    }
  });

  it('plans service team coverage in Rust with the owner scope values', async () => {
    const prepare = vi.fn(() => ({ bind: vi.fn() }));
    const batch = vi.fn(async () => [{ results: [], success: true, meta: { changes: 0, duration: 0 } }]);
    const db = { prepare, batch } as unknown as SqlDatabase;
    process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG = config;
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      ok('SELECT t.* FROM service_team_coverage c', ['company', 'it', 'company', null, null]),
    );
    try {
      const { resolveServiceTeam } = await import('@erpx/server-support/adapters/service-team-reads');
      const scope = { company_id: 'company', branch_id: null, department_id: null } as never;
      await expect(resolveServiceTeam(db, scope, 'it')).resolves.toBeNull();
      const call = fetchMock.mock.calls[0]!;
      expect(String(call[0])).toContain('/support/command-plan');
      expect(JSON.parse(String(call[1]!.body))).toMatchObject({
        kind: 'support_rd_service_team_coverage',
        values: ['company', 'it', 'company', null, null],
      });
    } finally {
      fetchMock.mockRestore();
      delete process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG;
    }
  });

  it('carries no retired ticket-portal response/acknowledgement SQL in the switched adapter', async () => {
    const portal = await source('packages/server-support/src/adapters/ticket-portal.ts');
    expect(portal).not.toContain('FROM support_ticket_portal_responses WHERE binding_id=?');
    expect(portal).not.toContain('FROM support_ticket_portal_acknowledgements WHERE binding_id=?');
    expect(portal).toContain('SUPPORT_RD_TICKET_PORTAL_RESPONSE');
    expect(portal).toContain('SUPPORT_RD_TICKET_PORTAL_ACKNOWLEDGED_AT');
  });

  it('plans the portal response and acknowledgement reads in Rust with owner-scoped binds', async () => {
    const prepare = vi.fn(() => ({ bind: vi.fn() }));
    const batch = vi.fn(async () => [
      { results: [{ id: 'response-1', response_version: 3, message: 'hello', created_at: 42 }], success: true, meta: { changes: 0, duration: 0 } }]);
    const db = { prepare, batch } as unknown as SqlDatabase;
    process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG = config;
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation(async () =>
      ok('SELECT id,response_version,message,created_at FROM support_ticket_portal_responses WHERE binding_id=?', ['binding-1']),
    );
    try {
      const { ticketPortalResponse, ticketPortalAcknowledgedAt } = await import('@erpx/server-support/adapters/ticket-portal');
      await expect(ticketPortalResponse(db, 'binding-1')).resolves.toEqual({ id: 'response-1', version: 3, message: 'hello', publishedAt: 42 });
      const responseCall = fetchMock.mock.calls[0]!;
      expect(String(responseCall[0])).toContain('/support/command-plan');
      expect(JSON.parse(String(responseCall[1]!.body))).toMatchObject({ kind: 'support_rd_ticket_portal_response', values: ['binding-1'] });

      fetchMock.mockClear();
      const ackBatch = vi.fn(async () => [
        { results: [{ created_at: 99 }], success: true, meta: { changes: 0, duration: 0 } }]);
      const ackDb = { prepare, batch: ackBatch } as unknown as SqlDatabase;
      await expect(ticketPortalAcknowledgedAt(ackDb, 'binding-1', 'response-1')).resolves.toBe(99);
      const ackCall = fetchMock.mock.calls[0]!;
      expect(JSON.parse(String(ackCall[1]!.body))).toMatchObject({ kind: 'support_rd_ticket_portal_acknowledged_at', values: ['binding-1', 'response-1'] });
      await expect(ticketPortalAcknowledgedAt(ackDb, 'binding-1')).resolves.toBeNull();
    } finally {
      fetchMock.mockRestore();
      delete process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG;
    }
  });

  it('carries no retired ticket/incident event-history SQL in the switched adapters', async () => {
    const detail = await source('packages/server-support/src/adapters/support.ts');
    const incidents = await source('packages/server-support/src/adapters/support-incidents.ts');
    expect(detail).not.toContain('FROM support_ticket_events WHERE ticket_id=?');
    expect(incidents).not.toContain('FROM support_incident_events WHERE incident_id=?');
    expect(detail).toContain('SUPPORT_RD_TICKET_EVENT_HISTORY');
    expect(incidents).toContain('SUPPORT_RD_INCIDENT_EVENT_HISTORY');
  });

  it('plans ticket and incident event history in Rust with owner-scoped binds', async () => {
    const batch = vi.fn(async () => [{ results: [{ id: 'e1', version: 2 }], success: true, meta: { changes: 0, duration: 0 } }]);
    const db = { prepare: vi.fn(() => ({ bind: vi.fn() })), batch } as unknown as SqlDatabase;
    process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG = config;
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation(async () =>
      ok('SELECT * FROM support_ticket_events WHERE ticket_id=? ORDER BY version DESC', ['ticket-1']),
    );
    try {
      const { nativeSupportReadAll, SUPPORT_RD_TICKET_EVENT_HISTORY, SUPPORT_RD_INCIDENT_EVENT_HISTORY } =
        await import('@erpx/server-support/adapters/native-support-read');
      await expect(nativeSupportReadAll(SUPPORT_RD_TICKET_EVENT_HISTORY, ['ticket-1'], db)
        .then((result: { results: unknown[] }) => result.results)).resolves.toEqual([{ id: 'e1', version: 2 }]);
      expect(JSON.parse(String(fetchMock.mock.calls[0]![1]!.body))).toMatchObject({
        kind: 'support_rd_ticket_event_history',
        values: ['ticket-1'],
      });

      fetchMock.mockClear();
      await expect(nativeSupportReadAll(SUPPORT_RD_INCIDENT_EVENT_HISTORY, ['incident-1'], db)
        .then((result: { results: unknown[] }) => result.results)).resolves.toEqual([{ id: 'e1', version: 2 }]);
      expect(JSON.parse(String(fetchMock.mock.calls[0]![1]!.body))).toMatchObject({
        kind: 'support_rd_incident_event_history',
        values: ['incident-1'],
      });
    } finally {
      fetchMock.mockRestore();
      delete process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG;
    }
  });

  it('carries no retired incident-ticket link SQL in the switched adapters', async () => {
    const detail = await source('packages/server-support/src/adapters/support.ts');
    const incidents = await source('packages/server-support/src/adapters/support-incidents.ts');
    expect(detail).not.toContain('FROM support_incident_tickets l JOIN support_incidents i');
    expect(incidents).not.toContain('FROM support_incident_tickets l JOIN support_tickets t');
    expect(detail).toContain('SUPPORT_RD_TICKET_INCIDENT_LINKS');
    expect(incidents).toContain('SUPPORT_RD_INCIDENT_TICKET_LINKS');
  });

  it('plans ticket and incident link reads in Rust with owner-scoped binds', async () => {
    const batch = vi.fn(async () => [{ results: [{ id: 'l1', title: 'Linked', status: 'open' }], success: true, meta: { changes: 0, duration: 0 } }]);
    const db = { prepare: vi.fn(() => ({ bind: vi.fn() })), batch } as unknown as SqlDatabase;
    process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG = config;
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation(async () =>
      ok('SELECT i.id,i.title,i.status FROM support_incident_tickets l', ['ticket-1']),
    );
    try {
      const { nativeSupportReadAll, SUPPORT_RD_TICKET_INCIDENT_LINKS, SUPPORT_RD_INCIDENT_TICKET_LINKS } =
        await import('@erpx/server-support/adapters/native-support-read');
      await expect(nativeSupportReadAll(SUPPORT_RD_TICKET_INCIDENT_LINKS, ['ticket-1'], db)
        .then((result: { results: unknown[] }) => result.results)).resolves.toEqual([{ id: 'l1', title: 'Linked', status: 'open' }]);
      expect(JSON.parse(String(fetchMock.mock.calls[0]![1]!.body))).toMatchObject({
        kind: 'support_rd_ticket_incident_links',
        values: ['ticket-1'],
      });

      fetchMock.mockClear();
      await expect(nativeSupportReadAll(SUPPORT_RD_INCIDENT_TICKET_LINKS, ['incident-1'], db)
        .then((result: { results: unknown[] }) => result.results)).resolves.toEqual([{ id: 'l1', title: 'Linked', status: 'open' }]);
      expect(JSON.parse(String(fetchMock.mock.calls[0]![1]!.body))).toMatchObject({
        kind: 'support_rd_incident_ticket_links',
        values: ['incident-1'],
      });
    } finally {
      fetchMock.mockRestore();
      delete process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG;
    }
  });
});
