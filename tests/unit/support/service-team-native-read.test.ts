import { readFile } from 'node:fs/promises';
import { describe, expect, it, vi } from 'vitest';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';

vi.mock('server-only', () => ({}));
vi.mock('@erpx/server-support/adapters/command-runtime', () => ({
  nativeSupportRuntime: () => ({ purchase: async () => ({ history: true }) }),
}));

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
    ]) expect(adapter).not.toContain(fragment);
    expect(adapter).toContain('SUPPORT_RD_SERVICE_TEAM_OPTIONS');
    expect(adapter).toContain('SUPPORT_RD_SERVICE_TEAM_OPTION_HISTORY');
    expect(adapter).toContain('SUPPORT_RD_SERVICE_TEAM_OPTION_HANDLERS');
    expect(adapter).toContain('SUPPORT_RD_SERVICE_TEAM_OPTION_COVERAGE');
    // Organization still owns the node projection list kept in TypeScript.
    expect(adapter).toContain('organization_nodes');
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
      } = await import('@erpx/server-support/adapters/native-support-read');
      const cases: [string, string[]][] = [
        [SUPPORT_RD_SERVICE_TEAM_OPTIONS, ['company']],
        [SUPPORT_RD_SERVICE_TEAM_OPTION_HISTORY, ['company', 'team-1']],
        [SUPPORT_RD_SERVICE_TEAM_OPTION_HANDLERS, ['team-1', 'company']],
        [SUPPORT_RD_SERVICE_TEAM_OPTION_COVERAGE, ['team-1', 'company']],
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
    expect(adapter).toContain('SUPPORT_RD_SLA_WORKSPACE_TICKETS');
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
});
