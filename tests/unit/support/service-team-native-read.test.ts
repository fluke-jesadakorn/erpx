import { readFile } from 'node:fs/promises';
import { describe, expect, it, vi } from 'vitest';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { supportRecipientEligibility } from '../../../packages/server-support/src/adapters/support-recipient';
import { supportEligibleFlagSql } from '../../../packages/server-support/src/adapters/eligibility-projection-sources';
import { scope as ticketScope } from '../../../packages/server-support/src/adapters/support-ticket-read-sources';
import { finalRootOwnerProjectionSources as identityAccessProjections } from '@erpx/server-identity_access/public/final-root-owner-projection-sources';
import { finalRootOwnerProjectionSources as payrollProjections } from '@erpx/server-people_payroll/public/final-root-owner-projection-sources';

vi.mock('server-only', () => ({}));
vi.mock('@erpx/server-support/adapters/command-runtime', () => ({
  nativeSupportRuntime: () => ({
    purchase: async () => ({ history: true }),
    requirePurchase: async () => ({ history: true }),
  }),
}));
const metricSourceMock = vi.hoisted(() => vi.fn(async (..._args: unknown[]) => ({ rows: [] })));
vi.mock('@erpx/server-support/adapters/ticket-source-read-policy', () => ({
  supportTicketSourceIdSets: async () => ({ posSale: ['pos-1'], asset: ['asset-1'], pmsRecord: [] }),
}));
vi.mock('@erpx/server-runtime/public/executive/metric-sources', () => ({
  readMetricSources: (...args: unknown[]) => metricSourceMock(...args),
}));
vi.mock('@erpx/server-commerce/public/support-attachment-sources', () => ({
  supportOrderAttachmentSourceLocks: () => [],
  supportOrderAttachmentSourceImage: async () => ({ id: 'order-1' }),
}));

const source = (path: string) => readFile(new URL(`../../../${path}`, import.meta.url), 'utf8');
/** Inlined oracle: the retired `supportReviewerCountSql` rendering. */
const reviewerCountSql = (predicate: string) =>
  `(SELECT COUNT(DISTINCT m.user_id) FROM (${identityAccessProjections.assignments}) m WHERE ${predicate})`;
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
    expect(detail).toContain("nativeSupportEventHistory<Row>(a, 'ticket', id)");
    expect(incidents).toContain("nativeSupportEventHistory<Row>(a, 'incident', id)");
    expect(detail).not.toContain('SUPPORT_RD_TICKET_EVENT_HISTORY');
    expect(incidents).not.toContain('SUPPORT_RD_INCIDENT_EVENT_HISTORY');
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

  it('reads history from the mounted native GET routes and fails closed', async () => {
    process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG = config;
    const rows = [{ id: 'e1', ticket_id: 'ticket-1', version: 1, action: 'create' }];
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation(async () =>
      new Response(JSON.stringify({ data: rows }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    );
    const actor = { holdingSlug: 'acme', holdingId: 'h1', companyId: 'company' };
    try {
      const { nativeSupportEventHistory } = await import('@erpx/server-support/adapters/native-support-read');
      await expect(nativeSupportEventHistory(actor as never, 'ticket', 'ticket-1')).resolves.toEqual(rows);
      expect(String(fetchMock.mock.calls[0]![0])).toBe(
        'http://127.0.0.1:3997/api/v1/holdings/acme/support/tickets/ticket-1/events',
      );
      fetchMock.mockClear();
      await expect(nativeSupportEventHistory(actor as never, 'incident', 'incident-1')).resolves.toEqual(rows);
      expect(String(fetchMock.mock.calls[0]![0])).toBe(
        'http://127.0.0.1:3997/api/v1/holdings/acme/support/incidents/incident-1/events',
      );
      fetchMock.mockClear();
      await expect(nativeSupportEventHistory(
        { ...actor, holdingId: 'another-holding' } as never, 'ticket', 'ticket-1',
      )).rejects.toMatchObject({ status: 422 });
      expect(fetchMock).not.toHaveBeenCalled();
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

  it('carries no retired ticket source-image SQL in the switched adapters', async () => {
    const preview = await source('packages/server-support/src/adapters/portal-preview-sources.ts');
    const attachment = await source('packages/server-support/src/adapters/attachment-read.ts');
    expect(preview).not.toContain('to_jsonb(ticket)::text image FROM support_tickets');
    expect(attachment).not.toContain('to_jsonb(t)::text image,t.source_record_json');
    expect(preview).toContain('SUPPORT_RD_TICKET_SOURCE_IMAGE');
    expect(attachment).toContain('SUPPORT_RD_TICKET_ATTACHMENT_SOURCE_IMAGE');
  });

  it('carries no retired attachment-migration scope SQL in the switched adapter', async () => {
    const adapter = await source('packages/server-support/src/adapters/attachment-migration-owner.ts');
    expect(adapter).not.toContain('SELECT');
    expect(adapter).toContain('SUPPORT_RD_ATTACHMENT_MIGRATION_SCOPE');
  });

  it('plans the attachment-migration scope read in Rust with the owner bind', async () => {
    const batch = vi.fn(async () => [{ results: [{
      company_id: 'company', branch_id: 'branch-1', department_id: 'dept-1', version: 7,
    }], success: true, meta: { changes: 0, duration: 0 } }]);
    const db = { prepare: vi.fn(() => ({ bind: vi.fn() })), batch } as unknown as SqlDatabase;
    process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG = config;
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation(async () =>
      ok('SELECT company_id,branch_id branch_id,department_id department_id,version FROM support_tickets WHERE id=?', ['ticket-1']),
    );
    try {
      const { legacyAttachmentScopesupport } = await import('@erpx/server-support/public/attachment-migration-owner');
      await expect(legacyAttachmentScopesupport(db, 'support_ticket', 'ticket-1')).resolves.toEqual({
        company_id: 'company', branch_id: 'branch-1', department_id: 'dept-1', version: 7,
      });
      const call = fetchMock.mock.calls[0]!;
      expect(String(call[0])).toContain('/support/command-plan');
      expect(JSON.parse(String(call[1]!.body))).toMatchObject({
        kind: 'support_rd_attachment_migration_scope',
        values: ['ticket-1'],
      });

      fetchMock.mockClear();
      await expect(legacyAttachmentScopesupport(db, 'unknown', 'ticket-1')).resolves.toBeNull();
      expect(fetchMock).not.toHaveBeenCalled();
    } finally {
      fetchMock.mockRestore();
      delete process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG;
    }
  });

  it('plans ticket source images in Rust with owner-scoped binds', async () => {
    const batch = vi.fn(async () => [{ results: [{ image: '{"id":"ticket-1"}', source_record_json: null, order_id: null, customer_id: null }], success: true, meta: { changes: 0, duration: 0 } }]);
    const db = { prepare: vi.fn(() => ({ bind: vi.fn() })), batch } as unknown as SqlDatabase;
    process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG = config;
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation(async () =>
      ok('SELECT to_jsonb(ticket)::text image FROM support_tickets ticket WHERE ticket.id=? AND ticket.company_id=?', ['ticket-1', 'company']),
    );
    try {
      const { portalPreviewSupportSourceImage } = await import('@erpx/server-support/public/portal-preview-sources');
      await expect(portalPreviewSupportSourceImage(db, 'company', 'ticket-1')).resolves.toBe('{"id":"ticket-1"}');
      expect(JSON.parse(String(fetchMock.mock.calls[0]![1]!.body))).toMatchObject({
        kind: 'support_rd_ticket_source_image',
        values: ['ticket-1', 'company'],
      });

      fetchMock.mockClear();
      const { nativeSupportReadAll, SUPPORT_RD_TICKET_ATTACHMENT_SOURCE_IMAGE } =
        await import('@erpx/server-support/adapters/native-support-read');
      await expect(nativeSupportReadAll<{ image: string }>(SUPPORT_RD_TICKET_ATTACHMENT_SOURCE_IMAGE, ['ticket-1', 'company'], db)
        .then((result: { results: unknown[] }) => result.results)).resolves.toEqual([{ image: '{"id":"ticket-1"}', source_record_json: null, order_id: null, customer_id: null }]);
      expect(JSON.parse(String(fetchMock.mock.calls[0]![1]!.body))).toMatchObject({
        kind: 'support_rd_ticket_attachment_source_image',
        values: ['ticket-1', 'company'],
      });
    } finally {
      fetchMock.mockRestore();
      delete process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG;
    }
  });

  it('carries no retired service-team option SQL in the switched adapter', async () => {
    const adapter = await source('packages/server-support/src/adapters/service-team-options.ts');
    for (const fragment of [
      'SELECT * FROM service_teams',
      'FROM service_team_revisions',
      'FROM service_team_members',
      'FROM service_team_coverage',
      'organization_nodes',
    ]) expect(adapter).not.toContain(fragment);
    expect(adapter).not.toContain('organizationProjectionSources');
    expect(adapter).toContain('SUPPORT_RD_SERVICE_TEAM_OPTIONS');
    expect(adapter).toContain('SUPPORT_RD_SERVICE_TEAM_OPTION_HISTORY');
    expect(adapter).toContain('SUPPORT_RD_SERVICE_TEAM_OPTION_HANDLERS');
    expect(adapter).toContain('SUPPORT_RD_SERVICE_TEAM_OPTION_COVERAGE');
    expect(adapter).toContain('SUPPORT_RD_SERVICE_TEAM_OPTION_NODES');
  });

  it('plans the four service-team option reads in Rust with owner-scoped binds', async () => {
    const batch = vi.fn(async () => [{ results: [{ id: 'team-1' }], success: true, meta: { changes: 0, duration: 0 } }]);
    const db = { prepare: vi.fn(() => ({ bind: vi.fn() })), batch } as unknown as SqlDatabase;
    process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG = config;
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation(async () =>
      ok('SELECT * FROM service_teams WHERE company_id=? ORDER BY name,id', ['company']),
    );
    try {
      const {
        nativeSupportReadAll,
        SUPPORT_RD_SERVICE_TEAM_OPTIONS,
        SUPPORT_RD_SERVICE_TEAM_OPTION_HISTORY,
        SUPPORT_RD_SERVICE_TEAM_OPTION_HANDLERS,
        SUPPORT_RD_SERVICE_TEAM_OPTION_COVERAGE,
        SUPPORT_RD_SERVICE_TEAM_OPTION_NODES,
      } = await import('@erpx/server-support/adapters/native-support-read');
      const cases: [string, string[]][] = [
        [SUPPORT_RD_SERVICE_TEAM_OPTIONS, ['company']],
        [SUPPORT_RD_SERVICE_TEAM_OPTION_HISTORY, ['company', 'team-1']],
        [SUPPORT_RD_SERVICE_TEAM_OPTION_HANDLERS, ['team-1', 'company']],
        [SUPPORT_RD_SERVICE_TEAM_OPTION_COVERAGE, ['team-1', 'company']],
        [SUPPORT_RD_SERVICE_TEAM_OPTION_NODES, ['company', 'branch', 'branch', 'branch', 'dept', 'dept']],
      ];
      for (const [kind, values] of cases) {
        fetchMock.mockClear();
        await nativeSupportReadAll(kind, values, db);
        expect(String(fetchMock.mock.calls[0]![0])).toContain('/support/command-plan');
        expect(JSON.parse(String(fetchMock.mock.calls[0]![1]!.body))).toMatchObject({ kind, values });
      }
    } finally {
      fetchMock.mockRestore();
      delete process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG;
    }
  });

  it('carries no retired incident ticket candidate SQL in the switched adapter', async () => {
    const adapter = await source('packages/server-support/src/adapters/support-incidents.ts');
    expect(adapter).not.toContain('source_kind=\'sales_order\' AND id NOT IN(SELECT ticket_id FROM support_incident_tickets');
    expect(adapter).toContain('SUPPORT_RD_INCIDENT_TICKET_CANDIDATES');
  });

  it('carries no retired SLA workspace ticket SQL in the switched adapter', async () => {
    const adapter = await source('packages/server-support/src/adapters/support-sla.ts');
    expect(adapter).not.toContain('sla_policy_version,sla_clock_json FROM support_tickets');
    expect(adapter).toContain('ERPX_NATIVE_SUPPORT_READ_CONFIG');
  });

  it('plans the SLA workspace ticket read in Rust with owner-scoped binds', async () => {
    const values = ['company', 'branch-1', 'branch-1', null, null];
    const batch = vi.fn(async () => [{ results: [{ id: 'ticket-1', status: 'open', sla_clock_json: '{}' }], success: true, meta: { changes: 0, duration: 0 } }]);
    const db = { prepare: vi.fn(() => ({ bind: vi.fn() })), batch } as unknown as SqlDatabase;
    process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG = config;
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation(async () =>
      ok('SELECT id,title,status,owner_id,sla_policy_id,sla_policy_version,sla_clock_json FROM support_tickets WHERE company_id=? AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?) AND sla_clock_json IS NOT NULL AND status NOT IN (\'closed\',\'cancelled\') ORDER BY updated_at DESC,id', values),
    );
    try {
      const { nativeSupportReadAll, SUPPORT_RD_SLA_WORKSPACE_TICKETS } =
        await import('@erpx/server-support/adapters/native-support-read');
      await expect(nativeSupportReadAll<{ id: string }>(SUPPORT_RD_SLA_WORKSPACE_TICKETS, values, db)
        .then((result: { results: unknown[] }) => result.results)).resolves.toEqual([{ id: 'ticket-1', status: 'open', sla_clock_json: '{}' }]);
      const call = fetchMock.mock.calls[0]!;
      expect(String(call[0])).toContain('/support/command-plan');
      expect(JSON.parse(String(call[1]!.body))).toMatchObject({
        kind: 'support_rd_sla_workspace_tickets',
        values,
      });
    } finally {
      fetchMock.mockRestore();
      delete process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG;
    }
  });

  it('plans the incident ticket candidate read in Rust with owner-scoped binds', async () => {
    const values = ['company', 'branch-1', null, 'incident-1'];
    const batch = vi.fn(async () => [{ results: [{ id: 'ticket-1', title: 'Ticket', version: 3 }], success: true, meta: { changes: 0, duration: 0 } }]);
    const db = { prepare: vi.fn(() => ({ bind: vi.fn() })), batch } as unknown as SqlDatabase;
    process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG = config;
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation(async () =>
      ok("SELECT id,title,version FROM support_tickets WHERE company_id=? AND branch_id=? AND department_id IS NOT DISTINCT FROM ?::text AND source_kind='sales_order' AND id NOT IN(SELECT ticket_id FROM support_incident_tickets WHERE incident_id=? AND active) ORDER BY updated_at DESC LIMIT 200", values),
    );
    try {
      const { nativeSupportReadAll, SUPPORT_RD_INCIDENT_TICKET_CANDIDATES } =
        await import('@erpx/server-support/adapters/native-support-read');
      await expect(nativeSupportReadAll<{ id: string; title: string; version: number }>(SUPPORT_RD_INCIDENT_TICKET_CANDIDATES, values, db)
        .then((result: { results: unknown[] }) => result.results)).resolves.toEqual([{ id: 'ticket-1', title: 'Ticket', version: 3 }]);
      const call = fetchMock.mock.calls[0]!;
      expect(String(call[0])).toContain('/support/command-plan');
      expect(JSON.parse(String(call[1]!.body))).toMatchObject({
        kind: 'support_rd_incident_ticket_candidates',
        values,
      });
    } finally {
      fetchMock.mockRestore();
      delete process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG;
    }
  });

  it('carries no retired package-obligation/Portal SQL in the switched adapter', async () => {
    const adapter = await source('packages/server-support/src/adapters/package-obligations.ts');
    expect(adapter).not.toContain('SELECT');
    for (const constant of [
      'SUPPORT_RD_PORTAL_BINDINGS_INSTALLED',
      'SUPPORT_RD_SUPPORT_PACKAGE_PORTAL_BINDINGS',
      'SUPPORT_RD_SUPPORT_PACKAGE_OBLIGATION_FACTS',
    ]) expect(adapter).toContain(constant);
  });

  it('plans the package-obligation/Portal family in Rust with owner-scoped binds', async () => {
    const factsValues = ['company', 'company', 'company', 42, 'company', 42];
    const portalValues = [42, 'company'];
    const batch = vi.fn(async () => [{ results: [{ id: 'binding-1', pending: true }], success: true, meta: { changes: 0, duration: 0 } }]);
    const db = { prepare: vi.fn(() => ({ bind: vi.fn() })), batch } as unknown as SqlDatabase;
    process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG = config;
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation(async () =>
      ok('SELECT binding.id FROM support_ticket_portal_bindings', portalValues),
    );
    try {
      const {
        nativeSupportReadAll,
        SUPPORT_RD_PORTAL_BINDINGS_INSTALLED,
        SUPPORT_RD_SUPPORT_PACKAGE_PORTAL_BINDINGS,
        SUPPORT_RD_SUPPORT_PACKAGE_OBLIGATION_FACTS,
      } = await import('@erpx/server-support/adapters/native-support-read');
      const cases: [string, unknown[]][] = [
        [SUPPORT_RD_PORTAL_BINDINGS_INSTALLED, []],
        [SUPPORT_RD_SUPPORT_PACKAGE_PORTAL_BINDINGS, portalValues],
        [SUPPORT_RD_SUPPORT_PACKAGE_OBLIGATION_FACTS, factsValues],
      ];
      for (const [kind, values] of cases) {
        fetchMock.mockClear();
        await expect(nativeSupportReadAll(kind, values, db)
          .then((result: { results: unknown[] }) => result.results)).resolves.toEqual([{ id: 'binding-1', pending: true }]);
        expect(String(fetchMock.mock.calls[0]![0])).toContain('/support/command-plan');
        expect(JSON.parse(String(fetchMock.mock.calls[0]![1]!.body))).toMatchObject({ kind, values });
      }
    } finally {
      fetchMock.mockRestore();
      delete process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG;
    }
  });

  it('leaves no retired owner SQL in the Support Calendar adapter', async () => {
    const calendar = await source('packages/server-support/src/adapters/calendar-support-facts.ts');
    expect(calendar).not.toContain('SELECT');
    expect(calendar).toContain('SUPPORT_RD_CALENDAR_SUPPORT_TICKETS');
    expect(calendar).toContain('SUPPORT_RD_CALENDAR_SUPPORT_INCIDENTS');
  });

  it('plans the calendar ticket feed in Rust with owner-scoped scalar binds', async () => {
    const batch = vi.fn(async () => [{ results: [{ id: 'row-1' }], success: true, meta: { changes: 0, duration: 0 } }]);
    const db = { prepare: vi.fn(() => ({ bind: vi.fn() })), batch } as unknown as SqlDatabase;
    process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG = config;
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(ok('SELECT 1', []));
    const actor = { assignmentId: 'assignment-1', userId: 'user-1', role: 'support', companyId: 'company', holdingId: 'h1' } as never;
    try {
      const { calendarSupportSupportFacts } = await import('@erpx/server-support/adapters/calendar-support-facts');
      const rows = await calendarSupportSupportFacts(db).read1({
        value1: 'Asia/Bangkok', value2: 'company', value3: null, value4: null,
        value5: '["company"]', value6: '2026-10-01', value7: '2026-11-01', value8: 'team',
        value9: 'user-1', value10: null, value11: '', value12: 100, actor,
      });
      expect(rows).toEqual([{ id: 'row-1' }]);
      expect(String(fetchMock.mock.calls[0]![0])).toContain('/support/command-plan');
      expect(JSON.parse(String(fetchMock.mock.calls[0]![1]!.body))).toMatchObject({
        kind: 'support_rd_calendar_support_tickets',
        values: ['Asia/Bangkok', 'company', null, null, null, null, '["company"]',
          'assignment-1', 'user-1', 'support', '2026-10-01', '2026-11-01', 'team',
          'user-1', 'user-1', null, null, '', '', 100],
      });
    } finally {
      fetchMock.mockRestore();
      delete process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG;
    }
  });

  it('plans the calendar incident feed in Rust with owner-scoped scalar binds', async () => {
    const batch = vi.fn(async () => [{ results: [{ id: 'row-1' }], success: true, meta: { changes: 0, duration: 0 } }]);
    const db = { prepare: vi.fn(() => ({ bind: vi.fn() })), batch } as unknown as SqlDatabase;
    process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG = config;
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(ok('SELECT 1', []));
    const actor = { assignmentId: 'assignment-1', userId: 'user-1', role: 'support', companyId: 'company', holdingId: 'h1' } as never;
    try {
      const { calendarSupportSupportFacts } = await import('@erpx/server-support/adapters/calendar-support-facts');
      const rows = await calendarSupportSupportFacts(db).read2({
        value1: 'company', value2: null, value3: null, value4: '["company"]',
        value5: '2026-10-01', value6: '2026-11-01', value7: 'team', value8: 'user-1',
        value9: null, value10: '', value11: 100, actor,
      });
      expect(rows).toEqual([{ id: 'row-1' }]);
      expect(JSON.parse(String(fetchMock.mock.calls[0]![1]!.body))).toMatchObject({
        kind: 'support_rd_calendar_support_incidents',
        values: ['company', null, null, null, null, '["company"]',
          'assignment-1', 'user-1', 'support', '2026-10-01', '2026-11-01', 'team',
          'user-1', 'user-1', null, null, '', '', 100],
      });
    } finally {
      fetchMock.mockRestore();
      delete process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG;
    }
  });

  it('leaves no retired owner SQL in the executive metric projection', async () => {
    const metric = await source('packages/server-support/src/adapters/executive-metric-projections.ts');
    expect(metric).not.toContain('SELECT');
    expect(metric).toContain('SUPPORT_RD_EXEC_TICKET_METRIC');
    expect(metric).toContain('SUPPORT_RD_EXEC_INCIDENT_METRIC');
  });

  it('plans the ticket metric in Rust with frozen source ids and scalar scope binds', async () => {
    const prepare = vi.fn(() => ({ bind: vi.fn() }));
    const batch = vi.fn(async () => [{
      results: [{ label: 'triage', value: '1' }], success: true, meta: { changes: 0, duration: 0 },
    }]);
    const db = { prepare, batch } as unknown as SqlDatabase;
    const plannedSql = "SELECT CASE WHEN ?='status' THEN t.status ELSE t.status END label,count(*)::text value FROM support_tickets t WHERE t.company_id=? GROUP BY 1 ORDER BY 1";
    process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG = config;
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => ok(plannedSql, []));
    metricSourceMock.mockClear();
    const actor = { assignmentId: 'assignment-1', userId: 'user-1', role: 'support', companyId: 'company', branchId: 'branch', departmentId: null } as never;
    try {
      const { readExecutiveMetricProjection } = await import('@erpx/server-support/adapters/executive-metric-projections');
      const snapshot = await readExecutiveMetricProjection(db, actor, 'open-support-tickets', { from: '2026-01-01', to: '2026-12-31', currency: 'THB' }, 'status');
      expect(snapshot.raw).toEqual([{ label: 'triage', value: '1' }]);
      const body = JSON.parse(String(fetchMock.mock.calls[0]![1]!.body));
      expect(body).toMatchObject({
        kind: 'support_rd_exec_ticket_metric',
        values: [
          'status', 'status', 'status', 0, 'company', 'branch', 'branch', null, null,
          'assignment-1', 'user-1', 'support', '["pos-1"]', '["asset-1"]', '[]', 0,
        ],
      });
      await snapshot.source({ label: 'triage', page: 1 });
      expect(metricSourceMock).toHaveBeenCalledWith(
        db, 'open-support-tickets', plannedSql, expect.any(Array),
        { label: 'triage', page: 1 }, snapshot.raw, expect.any(String), undefined,
      );
    } finally {
      fetchMock.mockRestore();
      delete process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG;
    }
  });

  it('plans the incident metric in Rust without a source predicate', async () => {
    const prepare = vi.fn(() => ({ bind: vi.fn() }));
    const batch = vi.fn(async () => [{
      results: [{ label: 'open', value: '1' }], success: true, meta: { changes: 0, duration: 0 },
    }]);
    const db = { prepare, batch } as unknown as SqlDatabase;
    process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG = config;
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation(async () =>
      ok('SELECT t.status label,count(*)::text value FROM support_incidents t GROUP BY 1 ORDER BY 1', []));
    const actor = { assignmentId: 'assignment-1', userId: 'user-1', role: 'support', companyId: 'company', branchId: 'branch', departmentId: null } as never;
    try {
      const { readExecutiveMetricProjection } = await import('@erpx/server-support/adapters/executive-metric-projections');
      const snapshot = await readExecutiveMetricProjection(db, actor, 'active-support-incidents', { from: '2026-01-01', to: '2026-12-31', currency: 'THB' }, 'status');
      expect(snapshot.raw).toEqual([{ label: 'open', value: '1' }]);
      const body = JSON.parse(String(fetchMock.mock.calls[0]![1]!.body));
      expect(body).toMatchObject({
        kind: 'support_rd_exec_incident_metric',
        values: [
          'status', 'status', 'status', 'status', 'company', 'branch', 'branch', null, null,
          'assignment-1', 'user-1', 'support',
        ],
      });
    } finally {
      fetchMock.mockRestore();
      delete process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG;
    }
  });

  it('plans the incident-attachment and Sales read tails in Rust with no local SELECT', async () => {
    const incident = await source('packages/server-support/src/adapters/incident-attachment-read-sources.ts');
    const sales = await source('packages/server-support/src/adapters/sales-projection.ts');
    const registry = await source('packages/server-support/src/adapters/native_support_read_sql.rs');
    const transport = await source('packages/server-support/src/adapters/native-support-read.ts');
    for (const text of [incident, sales]) expect(text).not.toContain('SELECT');
    expect(incident).toContain('LOCK TABLE support_incidents');
    expect(incident).toContain('supportOrderAttachmentSourceImage');
    for (const kind of [
      'support_rd_incident_attachment_source_image',
      'support_rd_incident_attachment_source_links',
      'support_rd_sales_support_counts',
      'support_rd_sales_open_support_cases',
      'support_rd_sales_customer_source_cases',
    ]) {
      expect(registry).toContain(kind);
      expect(transport).toContain(kind);
    }
    expect(sales).not.toContain('service-access');
  });

  it('rejects unknown kinds and wrong bind arity before transport', async () => {
    process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG = config;
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => ok('SELECT 1', []));
    const db = { prepare: vi.fn(() => ({ bind: vi.fn() })), batch: vi.fn() } as unknown as SqlDatabase;
    try {
      const { nativeSupportReadAll, SUPPORT_RD_SALES_SUPPORT_COUNTS } = await import(
        '@erpx/server-support/adapters/native-support-read'
      );
      await expect(nativeSupportReadAll(SUPPORT_RD_SALES_SUPPORT_COUNTS, ['company'], db))
        .rejects.toMatchObject({ status: 422, code: 'SUPPORT_NATIVE_READ_BINDS_INVALID' });
      await expect(nativeSupportReadAll('SELECT 1', [], db))
        .rejects.toMatchObject({ status: 422, code: 'SUPPORT_NATIVE_READ_BINDS_INVALID' });
      expect(fetchMock).not.toHaveBeenCalled();
    } finally {
      fetchMock.mockRestore();
      delete process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG;
    }
  });

  it('sends fixed scope/policy binds and one JSON id bind for the Sales projections', async () => {
    process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG = config;
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => ok('SELECT 1', []));
    const db = { prepare: vi.fn(() => ({ bind: vi.fn() })), batch: vi.fn(async () => [{ results: [], success: true, meta: { changes: 0, duration: 0 } }]) } as unknown as SqlDatabase;
    const actor = { companyId: 'company', branchId: 'branch', departmentId: 'dept', assignmentId: 'assignment', userId: 'user', role: 'role' } as never;
    try {
      const { salesSupportCounts, salesOpenSupportCases, supportCustomerSourceCases } = await import(
        '@erpx/server-support/adapters/sales-projection'
      );
      const valuesOf = (index: number) => JSON.parse(String(fetchMock.mock.calls[index]![1]!.body)).values;
      await salesSupportCounts(db, actor, ['customer-1', 'customer-2']);
      expect(valuesOf(0)).toEqual([
        'company', 'branch', 'branch', 'dept', 'dept',
        'company', 'branch', 'branch', 'dept', 'dept',
        'assignment', 'user', 'role', '["customer-1","customer-2"]',
      ]);
      await salesOpenSupportCases(db, actor, 'customer-1');
      expect(valuesOf(1)).toEqual([
        'company', 'branch', 'branch', 'dept', 'dept',
        'company', 'branch', 'branch', 'dept', 'dept',
        'assignment', 'user', 'role', 'customer-1',
      ]);
      await supportCustomerSourceCases(db, actor, 'customer-1');
      expect(valuesOf(2)).toEqual([
        'company', 'branch', 'branch', 'dept', 'dept',
        'assignment', 'user', 'role', '["pos-1"]', '["asset-1"]', '[]', 'customer-1',
      ]);
      expect(fetchMock.mock.calls.map(call => JSON.parse(String(call[1]!.body)).kind)).toEqual([
        'support_rd_sales_support_counts',
        'support_rd_sales_open_support_cases',
        'support_rd_sales_customer_source_cases',
      ]);
    } finally {
      fetchMock.mockRestore();
      delete process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG;
    }
  });

  it('plans the incident witness image and links without moving the Commerce source image', async () => {
    process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG = config;
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => ok('SELECT 1', []));
    const batch = vi.fn()
      .mockResolvedValueOnce([{ results: [{ image: '{}', events: null, recipients: null }], success: true, meta: { changes: 0, duration: 0 } }])
      .mockResolvedValueOnce([{ results: [{ image: '{}', ticket: '{}', order_id: 'order-1' }], success: true, meta: { changes: 0, duration: 0 } }]);
    const db = { prepare: vi.fn(() => ({ bind: vi.fn() })), batch } as unknown as SqlDatabase;
    const actor = { userId: 'user', companyId: 'company' } as never;
    try {
      const { supportIncidentAttachmentReadSourceImage } = await import(
        '@erpx/server-support/adapters/incident-attachment-read-sources'
      );
      const witness = await supportIncidentAttachmentReadSourceImage(db, actor, 'incident-1');
      expect(JSON.parse(String(witness))).toEqual([
        { image: '{}', events: null, recipients: null },
        [{ image: '{}', ticket: '{}', order_id: 'order-1', order: { id: 'order-1' } }],
      ]);
      expect(fetchMock.mock.calls.map(call => JSON.parse(String(call[1]!.body)).kind)).toEqual([
        'support_rd_incident_attachment_source_image',
        'support_rd_incident_attachment_source_links',
      ]);
      expect(JSON.parse(String(fetchMock.mock.calls[0]![1]!.body)).values).toEqual(['user', 'incident-1', 'company']);
      expect(JSON.parse(String(fetchMock.mock.calls[1]![1]!.body)).values).toEqual(['incident-1', 'company']);
      expect(batch).toHaveBeenCalledTimes(2);
    } finally {
      fetchMock.mockRestore();
      delete process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG;
    }
  });

  it('carries no retired SLA-alert or ticket-recipient SQL in the switched adapters', async () => {
    const alerts = await source('packages/server-support/src/adapters/support-sla-alerts.ts');
    const ticket = await source('packages/server-support/src/adapters/support.ts');
    expect(alerts).not.toContain('FROM support_sla_alerts s JOIN support_tickets t');
    expect(alerts).not.toContain('supportEligibleFlagSql(');
    expect(alerts).not.toContain('supportReviewerCountSql(');
    expect(alerts).toContain('SUPPORT_RD_SLA_ALERTS');
    expect(ticket).not.toContain('SELECT DISTINCT m.user_id id,p.legal_name name FROM support_tickets t JOIN');
    expect(ticket).toContain('SUPPORT_RD_TICKET_RECIPIENTS');
  });

  it('plans SLA alerts with the user id leading six owner binds', async () => {
    const values = ['user-1', 'company', 'branch-1', 'branch-1', 'dept-1', 'dept-1'];
    const batch = vi.fn(async () => [{
      results: [{ id: 'alert-1', can_ack: 1, recipients: 2 }],
      success: true,
      meta: { changes: 0, duration: 0 },
    }]);
    const db = { prepare: vi.fn(() => ({ bind: vi.fn() })), batch } as unknown as SqlDatabase;
    process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG = config;
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      ok('SELECT s.* FROM support_sla_alerts s', values),
    );
    try {
      const { nativeSupportReadAll, SUPPORT_RD_SLA_ALERTS } =
        await import('@erpx/server-support/adapters/native-support-read');
      const result = await nativeSupportReadAll<{ id: string }>(SUPPORT_RD_SLA_ALERTS, values, db);
      expect(result.results).toEqual([{ id: 'alert-1', can_ack: 1, recipients: 2 }]);
      expect(String(fetchMock.mock.calls[0]![0])).toContain('/support/command-plan');
      expect(JSON.parse(String(fetchMock.mock.calls[0]![1]!.body))).toMatchObject({
        kind: 'support_rd_sla_alerts',
        values,
      });
      await expect(nativeSupportReadAll(SUPPORT_RD_SLA_ALERTS, values.slice(0, 5), db))
        .rejects.toMatchObject({ status: 422, code: 'SUPPORT_NATIVE_READ_BINDS_INVALID' });
    } finally {
      fetchMock.mockRestore();
      delete process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG;
    }
  });

  it('plans ticket recipients with the ticket id leading six owner binds', async () => {
    const values = ['ticket-1', 'company', 'branch-1', 'branch-1', 'dept-1', 'dept-1'];
    const batch = vi.fn(async () => [{
      results: [{ id: 'user-1', name: 'Support Manager' }],
      success: true,
      meta: { changes: 0, duration: 0 },
    }]);
    const db = { prepare: vi.fn(() => ({ bind: vi.fn() })), batch } as unknown as SqlDatabase;
    process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG = config;
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      ok('SELECT DISTINCT m.user_id id,p.legal_name name FROM support_tickets t', values),
    );
    try {
      const { nativeSupportReadAll, SUPPORT_RD_TICKET_RECIPIENTS } =
        await import('@erpx/server-support/adapters/native-support-read');
      const result = await nativeSupportReadAll<{ id: string; name: string }>(SUPPORT_RD_TICKET_RECIPIENTS, values, db);
      expect(result.results).toEqual([{ id: 'user-1', name: 'Support Manager' }]);
      expect(String(fetchMock.mock.calls[0]![0])).toContain('/support/command-plan');
      expect(JSON.parse(String(fetchMock.mock.calls[0]![1]!.body))).toMatchObject({
        kind: 'support_rd_ticket_recipients',
        values,
      });
      await expect(nativeSupportReadAll(SUPPORT_RD_TICKET_RECIPIENTS, values.slice(0, 5), db))
        .rejects.toMatchObject({ status: 422, code: 'SUPPORT_NATIVE_READ_BINDS_INVALID' });
    } finally {
      fetchMock.mockRestore();
      delete process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG;
    }
  });

  it('keeps the retired SLA-alert and ticket-recipient SQL byte-identical in the Rust registry', async () => {
    const registry = await source('packages/server-support/src/adapters/native_support_read_sql.rs');
    const eligibility = supportRecipientEligibility().replaceAll('t.owner_id', 'm.user_id');
    const recipient = 'm.company_id=t.company_id AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)'
      + " AND iam_role_allows(m.role,m.company_id,'support.manage')"
      + " AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=t.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=t.department_id))"
      + ' AND ' + eligibility;
    const sla = `SELECT s.*,t.title,t.company_id,t.branch_id,t.department_id, ${supportEligibleFlagSql(recipient, 'can_ack')}, ${reviewerCountSql(recipient)} recipients FROM support_sla_alerts s JOIN support_tickets t ON t.id=s.ticket_id WHERE ${ticketScope} ORDER BY s.breached_at,s.id`;
    const recipients = `SELECT DISTINCT m.user_id id,p.legal_name name FROM support_tickets t JOIN (${identityAccessProjections.assignments}) m ON m.company_id=t.company_id JOIN (${payrollProjections.hr_people}) p ON p.company_id=t.company_id AND p.user_id=m.user_id WHERE t.id=? AND ${ticketScope} AND ${eligibility} ORDER BY p.legal_name`;
    for (const sql of [sla, recipients]) {
      expect(sql.match(/\?/g)).toHaveLength(6);
      expect(registry).toContain(`"${sql.replaceAll('"', '\\"')}"`);
    }
    expect(registry).toContain('support_rd_sla_alerts');
    expect(registry).toContain('support_rd_ticket_recipients');
  });
});
