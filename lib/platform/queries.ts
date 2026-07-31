import 'server-only';

import { randomUUID } from 'node:crypto';
import { controlReadQuery, withTransaction } from '@/db';
import { enqueueBridgeEvent } from './bridge-write';
import { boundedStringArray, isUuid } from './input';
import type {
  PlatformCustomerDetail,
  PlatformCustomerRow,
  PlatformEntitlement,
  PlatformHolding,
  PlatformOverview,
  PlatformRoleAssignment,
  PlatformTicket,
  SupportMode,
  TicketSeverity,
  TicketStatus,
} from './types';

export interface PlatformCustomerFilters {
  search?: string;
  subscription?: string;
  health?: string;
  risk?: 'roles' | 'support' | '';
}

function healthExpression(): string {
  return `CASE
    WHEN ca.worker_health = 'down' OR ca.integration_failures >= 5 THEN 'down'
    WHEN ca.worker_health = 'degraded' OR ca.projection_lag_seconds > 60 OR ca.integration_failures > 0 THEN 'degraded'
    ELSE ca.worker_health
  END`;
}

function customerSelect(): string {
  return `
    ca.id::text,
    ca.account_code,
    ca.trading_name,
    ca.legal_name,
    ca.industry,
    ca.country,
    ca.status,
    ca.subscription_state,
    ca.payment_status,
    ca.renewal_at::text,
    ca.lifetime_value::text,
    ca.onboarding_stage,
    ca.training_status,
    ca.data_import_status,
    ca.go_live_date::text,
    ca.customer_success_owner,
    ca.database_version,
    ca.projection_lag_seconds::float8,
    ca.worker_health,
    ca.storage_bytes::bigint,
    ca.backup_status,
    ca.integration_failures,
    ca.api_usage_30d::bigint,
    ca.last_activity_at::text,
    count(DISTINCT h.id)::int AS holdings_count,
    count(DISTINCT co.id)::int AS companies_count,
    count(DISTINCT b.id)::int AS branches_count,
    count(DISTINCT cu.id) FILTER (WHERE cu.status = 'active')::int AS active_users,
    count(DISTINCT ra.id) FILTER (
      WHERE ra.revoked_at IS NULL
        AND ra.valid_until > now()
        AND ra.valid_until <= now() + interval '30 days'
    )::int AS expiring_roles,
    count(DISTINCT e.module_key) FILTER (WHERE e.status IN ('active', 'scheduled'))::int AS module_count,
    count(DISTINCT st.id) FILTER (WHERE st.status NOT IN ('closed', 'customer_confirmed'))::int AS open_tickets,
    COALESCE(MAX(CASE st.severity WHEN 'sev1' THEN 1 WHEN 'sev2' THEN 2 WHEN 'sev3' THEN 3 WHEN 'sev4' THEN 4 ELSE 5 END), 5)::int AS support_severity_rank,
    ${healthExpression()} AS health`;
}

function mapCustomer(row: Record<string, unknown>): PlatformCustomerRow {
  const severityRank = Number(row.support_severity_rank ?? 5);
  const supportSeverity: PlatformCustomerRow['support_severity'] =
    severityRank === 1 ? 'sev1' : severityRank === 2 ? 'sev2' : severityRank === 3 ? 'sev3' : severityRank === 4 ? 'sev4' : 'none';
  return {
    id: String(row.id),
    account_code: String(row.account_code),
    trading_name: String(row.trading_name),
    legal_name: row.legal_name == null ? null : String(row.legal_name),
    industry: row.industry == null ? null : String(row.industry),
    country: String(row.country),
    status: row.status as PlatformCustomerRow['status'],
    subscription_state: row.subscription_state as PlatformCustomerRow['subscription_state'],
    payment_status: String(row.payment_status),
    renewal_at: row.renewal_at == null ? null : String(row.renewal_at),
    lifetime_value: String(row.lifetime_value ?? '0'),
    onboarding_stage: String(row.onboarding_stage),
    training_status: String(row.training_status),
    data_import_status: String(row.data_import_status),
    go_live_date: row.go_live_date == null ? null : String(row.go_live_date),
    customer_success_owner: row.customer_success_owner == null ? null : String(row.customer_success_owner),
    database_version: row.database_version == null ? null : String(row.database_version),
    projection_lag_seconds: Number(row.projection_lag_seconds ?? 0),
    worker_health: row.worker_health as PlatformCustomerRow['worker_health'],
    storage_bytes: Number(row.storage_bytes ?? 0),
    backup_status: String(row.backup_status),
    integration_failures: Number(row.integration_failures ?? 0),
    api_usage_30d: Number(row.api_usage_30d ?? 0),
    last_activity_at: row.last_activity_at == null ? null : String(row.last_activity_at),
    holdings_count: Number(row.holdings_count ?? 0),
    companies_count: Number(row.companies_count ?? 0),
    branches_count: Number(row.branches_count ?? 0),
    active_users: Number(row.active_users ?? 0),
    expiring_roles: Number(row.expiring_roles ?? 0),
    module_count: Number(row.module_count ?? 0),
    open_tickets: Number(row.open_tickets ?? 0),
    support_severity: supportSeverity,
    health: row.health as PlatformCustomerRow['health'],
  };
}

function ticketFromRow(row: Record<string, unknown>): PlatformTicket {
  return {
    id: String(row.id),
    ticket_number: String(row.ticket_number),
    customer_account_id: String(row.customer_account_id),
    module_key: row.module_key == null ? null : String(row.module_key),
    environment: String(row.environment),
    severity: row.severity as TicketSeverity,
    category: String(row.category),
    affected_users: Number(row.affected_users ?? 0),
    description: String(row.description),
    diagnostics_consent: Boolean(row.diagnostics_consent),
    assigned_team: row.assigned_team == null ? null : String(row.assigned_team),
    assigned_agent: row.assigned_agent == null ? null : String(row.assigned_agent),
    status: row.status as TicketStatus,
    created_by: String(row.created_by),
    created_at: String(row.created_at),
    updated_at: String(row.updated_at),
  };
}

function roleFromRow(row: Record<string, unknown>): PlatformRoleAssignment {
  const scope = [row.holding_name, row.company_name, row.branch_name].filter(Boolean).join(' / ');
  return {
    id: String(row.id),
    principal_ref: String(row.principal_ref),
    principal_type: String(row.principal_type),
    role_key: String(row.role_key),
    functional_domain: String(row.functional_domain),
    authority_level: Number(row.authority_level ?? 99),
    scope_label: scope || 'Platform-wide',
    valid_from: String(row.valid_from),
    valid_until: String(row.valid_until),
    risk_tier: String(row.risk_tier),
    grant_reason: String(row.grant_reason),
    parent_assignment_id: row.parent_assignment_id == null ? null : String(row.parent_assignment_id),
    assignment_version: Number(row.assignment_version ?? 1),
    effective: Boolean(row.effective),
  };
}

export async function listPlatformCustomers(filters: PlatformCustomerFilters = {}): Promise<PlatformCustomerRow[]> {
  const values: unknown[] = [];
  const conditions = ['1 = 1'];
  if (filters.search) {
    values.push(`%${filters.search}%`);
    conditions.push(`(ca.trading_name ILIKE $${values.length} OR ca.account_code ILIKE $${values.length} OR ca.legal_name ILIKE $${values.length})`);
  }
  if (filters.subscription && filters.subscription !== 'all') {
    values.push(filters.subscription);
    conditions.push(`ca.subscription_state = $${values.length}`);
  }
  if (filters.health && filters.health !== 'all') {
    values.push(filters.health);
    conditions.push(`${healthExpression()} = $${values.length}`);
  }
  const having: string[] = [];
  if (filters.risk === 'roles') {
    having.push(`count(DISTINCT ra.id) FILTER (WHERE ra.revoked_at IS NULL AND ra.valid_until > now() AND ra.valid_until <= now() + interval '30 days') > 0`);
  }
  if (filters.risk === 'support') {
    having.push(`count(DISTINCT st.id) FILTER (WHERE st.status NOT IN ('closed', 'customer_confirmed')) > 0`);
  }
  const result = await controlReadQuery<Record<string, unknown>>(
    `SELECT ${customerSelect()}
       FROM platform.customer_accounts ca
       LEFT JOIN platform.holdings h ON h.customer_account_id = ca.id
       LEFT JOIN platform.companies co ON co.holding_id = h.id
       LEFT JOIN platform.branches b ON b.company_id = co.id
       LEFT JOIN platform.customer_users cu ON cu.customer_account_id = ca.id
       LEFT JOIN platform.entitlements e ON e.customer_account_id = ca.id
       LEFT JOIN platform.support_tickets st ON st.customer_account_id = ca.id
       LEFT JOIN platform.role_assignments ra ON ra.holding_id = h.id
      WHERE ${conditions.join(' AND ')}
      GROUP BY ca.id
      ${having.length ? `HAVING ${having.join(' AND ')}` : ''}
      ORDER BY CASE ca.subscription_state WHEN 'past_due' THEN 1 WHEN 'grace' THEN 2 WHEN 'read_only' THEN 3 ELSE 4 END,
               ca.renewal_at NULLS LAST, ca.trading_name
      LIMIT 200`,
    values,
  );
  return result.rows.map(mapCustomer);
}

export async function listPlatformTickets(limit = 20): Promise<PlatformTicket[]> {
  const result = await controlReadQuery<Record<string, unknown>>(
    `SELECT id::text, ticket_number, customer_account_id::text, module_key, environment,
            severity, category, affected_users, description, diagnostics_consent,
            assigned_team, assigned_agent, status, created_by, created_at::text, updated_at::text
       FROM platform.support_tickets
      WHERE status NOT IN ('closed', 'customer_confirmed')
      ORDER BY CASE severity WHEN 'sev1' THEN 1 WHEN 'sev2' THEN 2 WHEN 'sev3' THEN 3 ELSE 4 END,
               updated_at DESC
      LIMIT $1`,
    [limit],
  );
  return result.rows.map(ticketFromRow);
}

export async function listEffectiveRoleAssignments(limit = 30): Promise<PlatformRoleAssignment[]> {
  const result = await controlReadQuery<Record<string, unknown>>(
    `WITH RECURSIVE effective AS (
       SELECT ra.*, (ra.valid_from <= now() AND ra.valid_until > now() AND ra.revoked_at IS NULL) AS effective_now
         FROM platform.role_assignments ra
        WHERE ra.parent_assignment_id IS NULL
       UNION ALL
       SELECT child.*, (parent.effective_now AND child.valid_from <= now() AND child.valid_until > now() AND child.revoked_at IS NULL) AS effective_now
         FROM platform.role_assignments child
         JOIN effective parent ON parent.id = child.parent_assignment_id
     )
     SELECT e.id::text, e.principal_ref, e.principal_type, e.role_key, e.functional_domain,
            e.authority_level, e.valid_from::text, e.valid_until::text, e.risk_tier,
            e.grant_reason, e.parent_assignment_id::text, e.assignment_version,
            e.effective_now AS effective,
            h.name AS holding_name, co.legal_name AS company_name, b.name AS branch_name
       FROM effective e
       LEFT JOIN platform.holdings h ON h.id = e.holding_id
       LEFT JOIN platform.companies co ON co.id = e.company_id
       LEFT JOIN platform.branches b ON b.id = e.branch_id
      WHERE e.revoked_at IS NULL
      ORDER BY e.valid_until ASC
      LIMIT $1`,
    [limit],
  );
  return result.rows.map(roleFromRow);
}

export async function loadPlatformOverview(filters: PlatformCustomerFilters = {}): Promise<PlatformOverview> {
  const [customers, tickets, roles, metrics] = await Promise.all([
    listPlatformCustomers(filters),
    listPlatformTickets(12),
    listEffectiveRoleAssignments(24),
    controlReadQuery<{ total_customers: number; active_customers: number; payment_risk: number; open_tickets: number; roles_expiring_30d: number }>(
      `SELECT
         count(*)::int AS total_customers,
         count(*) FILTER (WHERE status = 'active')::int AS active_customers,
         count(*) FILTER (WHERE subscription_state IN ('past_due', 'grace', 'read_only'))::int AS payment_risk,
         (SELECT count(*)::int FROM platform.support_tickets WHERE status NOT IN ('closed', 'customer_confirmed')) AS open_tickets,
         (SELECT count(*)::int FROM platform.role_assignments WHERE revoked_at IS NULL AND valid_until > now() AND valid_until <= now() + interval '30 days') AS roles_expiring_30d
       FROM platform.customer_accounts`,
    ),
  ]);
  return { customers, tickets, roles, metrics: metrics.rows[0] };
}

export async function getPlatformCustomer(id: string): Promise<PlatformCustomerDetail | null> {
  const account = await controlReadQuery<Record<string, unknown>>(
    `SELECT ${customerSelect()}
       FROM platform.customer_accounts ca
       LEFT JOIN platform.holdings h ON h.customer_account_id = ca.id
       LEFT JOIN platform.companies co ON co.holding_id = h.id
       LEFT JOIN platform.branches b ON b.company_id = co.id
       LEFT JOIN platform.customer_users cu ON cu.customer_account_id = ca.id
       LEFT JOIN platform.entitlements e ON e.customer_account_id = ca.id
       LEFT JOIN platform.support_tickets st ON st.customer_account_id = ca.id
       LEFT JOIN platform.role_assignments ra ON ra.holding_id = h.id
      WHERE ca.id = $1
      GROUP BY ca.id`,
    [id],
  );
  const row = account.rows[0];
  if (!row) return null;
  const [holdings, entitlements, tickets, roles, accessAudit] = await Promise.all([
    controlReadQuery<Record<string, unknown>>(
      `SELECT h.id::text, h.name, h.owner_contact, h.security_contact, h.status,
              COALESCE(jsonb_agg(jsonb_build_object(
                'id', co.id::text, 'legal_name', co.legal_name, 'tax_id', co.tax_id,
                'status', co.status, 'branches', COALESCE((
                  SELECT jsonb_agg(jsonb_build_object('id', b.id::text, 'branch_code', b.branch_code, 'name', b.name, 'status', b.status) ORDER BY b.branch_code)
                    FROM platform.branches b WHERE b.company_id = co.id
                ), '[]'::jsonb)
              ) ORDER BY co.legal_name) FILTER (WHERE co.id IS NOT NULL), '[]'::jsonb) AS companies
         FROM platform.holdings h
         LEFT JOIN platform.companies co ON co.holding_id = h.id
        WHERE h.customer_account_id = $1
        GROUP BY h.id
        ORDER BY h.name`,
      [id],
    ),
    controlReadQuery<Record<string, unknown>>(
      `SELECT module_key, status, seats, company_limit, branch_limit, included_usage::text, active_until::text
         FROM platform.entitlements WHERE customer_account_id = $1 ORDER BY module_key`,
      [id],
    ),
    controlReadQuery<Record<string, unknown>>(
      `SELECT id::text, ticket_number, customer_account_id::text, module_key, environment,
              severity, category, affected_users, description, diagnostics_consent,
              assigned_team, assigned_agent, status, created_by, created_at::text, updated_at::text
         FROM platform.support_tickets WHERE customer_account_id = $1 ORDER BY updated_at DESC LIMIT 40`,
      [id],
    ),
    controlReadQuery<Record<string, unknown>>(
      `SELECT ra.id::text, ra.principal_ref, ra.principal_type, ra.role_key, ra.functional_domain,
              ra.authority_level, ra.valid_from::text, ra.valid_until::text, ra.risk_tier,
              ra.grant_reason, ra.parent_assignment_id::text, ra.assignment_version,
              (ra.valid_from <= now() AND ra.valid_until > now() AND ra.revoked_at IS NULL) AS effective,
              h.name AS holding_name, co.legal_name AS company_name, b.name AS branch_name
         FROM platform.role_assignments ra
         LEFT JOIN platform.holdings h ON h.id = ra.holding_id
         LEFT JOIN platform.companies co ON co.id = ra.company_id
         LEFT JOIN platform.branches b ON b.id = ra.branch_id
        WHERE ra.holding_id IN (SELECT id FROM platform.holdings WHERE customer_account_id = $1)
           OR ra.principal_type = 'platform_user'
        ORDER BY ra.valid_until ASC LIMIT 60`,
      [id],
    ),
    controlReadQuery<{ action: string; module_key: string | null; purpose: string; correlation_id: string; created_at: string }>(
      `SELECT action, module_key, purpose, correlation_id, created_at::text
         FROM platform.support_access_audit
        WHERE customer_account_id = $1
        ORDER BY created_at DESC LIMIT 30`,
      [id],
    ),
  ]);

  const mappedHoldings: PlatformHolding[] = holdings.rows.map((holding) => ({
    id: String(holding.id),
    name: String(holding.name),
    owner_contact: holding.owner_contact == null ? null : String(holding.owner_contact),
    security_contact: holding.security_contact == null ? null : String(holding.security_contact),
    status: String(holding.status),
    companies: Array.isArray(holding.companies) ? holding.companies as PlatformHolding['companies'] : [],
  }));
  const mappedEntitlements: PlatformEntitlement[] = entitlements.rows.map((item) => ({
    module_key: String(item.module_key),
    status: String(item.status),
    seats: Number(item.seats ?? 0),
    company_limit: Number(item.company_limit ?? 0),
    branch_limit: Number(item.branch_limit ?? 0),
    included_usage: String(item.included_usage ?? '0'),
    active_until: item.active_until == null ? null : String(item.active_until),
  }));
  return {
    customer: mapCustomer(row),
    holdings: mappedHoldings,
    entitlements: mappedEntitlements,
    tickets: tickets.rows.map(ticketFromRow),
    roles: roles.rows.map(roleFromRow),
    access_audit: accessAudit.rows,
  };
}

export async function recordGlobalRead(args: {
  customerId: string;
  actorId: number;
  moduleKey: string;
  purpose: string;
  companyId?: string | null;
  branchId?: string | null;
}): Promise<string> {
  const correlationId = randomUUID();
  await withTransaction(async (tx) => {
    await tx(
      `INSERT INTO platform.support_access_audit
         (customer_account_id, platform_actor_id, action, module_key, company_id, branch_id, purpose, correlation_id)
       VALUES ($1, $2, 'global_read', $3, $4, $5, $6, $7)`,
      [args.customerId, args.actorId, args.moduleKey, args.companyId ?? null, args.branchId ?? null, args.purpose, correlationId],
    );
    await enqueueBridgeEvent(tx, {
      eventType: 'global_read_recorded',
      aggregateType: 'customer_account',
      aggregateId: args.customerId,
      aggregateVersion: Date.now(),
      idempotencyKey: `folio-global-read-${correlationId}`,
      payload: {
        customerId: args.customerId,
        actorId: args.actorId,
        moduleKey: args.moduleKey,
        companyId: args.companyId ?? null,
        branchId: args.branchId ?? null,
        purpose: args.purpose,
        correlationId,
      },
    });
  });
  return correlationId;
}

export async function createPlatformTicket(args: {
  customerId: string;
  moduleKey?: string | null;
  environment?: 'production' | 'staging' | 'sandbox';
  severity?: TicketSeverity;
  category?: string;
  affectedUsers?: number;
  description: string;
  diagnosticsConsent?: boolean;
  createdBy: string;
}): Promise<PlatformTicket> {
  const ticketNumber = `ERPX-${new Date().getUTCFullYear()}-${randomUUID().slice(0, 8).toUpperCase()}`;
  return withTransaction(async (tx) => {
    const result = await tx<Record<string, unknown>>(
      `INSERT INTO platform.support_tickets
        (ticket_number, customer_account_id, module_key, environment, severity, category, affected_users, description, diagnostics_consent, created_by)
       VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
       RETURNING id::text, ticket_number, customer_account_id::text, module_key, environment,
                 severity, category, affected_users, description, diagnostics_consent,
                 assigned_team, assigned_agent, status, created_by, created_at::text, updated_at::text`,
      [ticketNumber, args.customerId, args.moduleKey ?? null, args.environment ?? 'production', args.severity ?? 'sev3', args.category ?? 'question', args.affectedUsers ?? 0, args.description, args.diagnosticsConsent ?? false, args.createdBy],
    );
    const ticket = ticketFromRow(result.rows[0]);
    await enqueueBridgeEvent(tx, {
      eventType: 'support_ticket_created',
      aggregateType: 'support_ticket',
      aggregateId: ticket.id,
      aggregateVersion: 1,
      idempotencyKey: `folio-support-ticket-created-${ticket.id}`,
      payload: {
        ticketId: ticket.id,
        ticketNumber: ticket.ticket_number,
        customerId: ticket.customer_account_id,
        moduleKey: ticket.module_key,
        environment: ticket.environment,
        severity: ticket.severity,
        category: ticket.category,
        affectedUsers: ticket.affected_users,
        status: ticket.status,
        createdBy: ticket.created_by,
      },
    });
    return ticket;
  });
}

function maxSupportMinutes(mode: SupportMode): number {
  return mode === 'diagnostic_read' ? 480 : 120;
}

export async function requestSupportSession(args: {
  ticketId: string;
  customerId: string;
  agentId: number;
  agentName: string;
  representedTenantUser?: string | null;
  mode: SupportMode;
  permissions?: string[];
  scope?: Record<string, unknown>;
  reason: string;
  durationMinutes: number;
}): Promise<{ id: string; status: string; expires_at: string }> {
  const duration = Math.floor(args.durationMinutes);
  if (!Number.isFinite(duration) || duration < 1 || duration > maxSupportMinutes(args.mode)) {
    throw new Error(`support session duration exceeds the ${maxSupportMinutes(args.mode)} minute policy`);
  }
  if (args.mode === 'impersonation' && !args.representedTenantUser) {
    throw new Error('impersonation requires an explicitly represented tenant user');
  }
  return withTransaction(async (tx) => {
    const ticket = await tx<{ customer_account_id: string; status: TicketStatus; company_id: string | null; branch_id: string | null }>(
      `SELECT customer_account_id::text, status, company_id::text, branch_id::text
         FROM platform.support_tickets WHERE id = $1 FOR UPDATE`,
      [args.ticketId],
    );
    if (!ticket.rows[0]) throw new Error('support ticket not found');
    if (ticket.rows[0].customer_account_id !== args.customerId) throw new Error('ticket/customer scope mismatch');
    if (ticket.rows[0].status === 'closed') throw new Error('closed tickets cannot receive support sessions');
    const requestedScope = args.scope ?? {};
    const requestedCompanyId = requestedScope.companyId == null ? null : String(requestedScope.companyId).trim();
    const requestedBranchId = requestedScope.branchId == null ? null : String(requestedScope.branchId).trim();
    if ((requestedCompanyId && !isUuid(requestedCompanyId)) || (requestedBranchId && !isUuid(requestedBranchId))) {
      throw new Error('support scope identifiers must be UUIDs');
    }
    if (requestedCompanyId && ticket.rows[0].company_id && requestedCompanyId !== ticket.rows[0].company_id) {
      throw new Error('support scope exceeds the ticket company');
    }
    if (requestedBranchId && ticket.rows[0].branch_id && requestedBranchId !== ticket.rows[0].branch_id) {
      throw new Error('support scope exceeds the ticket branch');
    }
    if (requestedCompanyId && !ticket.rows[0].company_id) {
      const company = await tx<{ allowed: boolean }>(
        `SELECT EXISTS(
           SELECT 1 FROM platform.companies co
           JOIN platform.holdings h ON h.id = co.holding_id
          WHERE co.id = $1 AND h.customer_account_id = $2
        ) AS allowed`,
        [requestedCompanyId, args.customerId],
      );
      if (!company.rows[0]?.allowed) throw new Error('support scope is outside the customer');
    }
    if (requestedBranchId && !ticket.rows[0].branch_id) {
      const branch = await tx<{ allowed: boolean }>(
        `SELECT EXISTS(
           SELECT 1 FROM platform.branches b
           JOIN platform.companies co ON co.id = b.company_id
           JOIN platform.holdings h ON h.id = co.holding_id
          WHERE b.id = $1 AND h.customer_account_id = $2
            AND ($3::uuid IS NULL OR co.id = $3)
        ) AS allowed`,
        [requestedBranchId, args.customerId, requestedCompanyId],
      );
      if (!branch.rows[0]?.allowed) throw new Error('support scope is outside the customer');
    }
    const scope = {
      ticket: args.ticketId,
      modules: boundedStringArray(requestedScope.modules, 20, 120),
      companyId: ticket.rows[0].company_id ?? requestedCompanyId,
      branchId: ticket.rows[0].branch_id ?? requestedBranchId,
    };
    const result = await tx<{ id: string; status: string; expires_at: string }>(
      `INSERT INTO platform.support_sessions
        (ticket_id, customer_account_id, agent_user_id, agent_name, represented_tenant_user, mode,
         permissions, scope, reason, requested_duration_minutes, expires_at)
       VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, now() + ($10::text || ' minutes')::interval)
       RETURNING id::text, status, expires_at::text`,
      [args.ticketId, args.customerId, args.agentId, args.agentName, args.representedTenantUser ?? null, args.mode, args.permissions ?? [], JSON.stringify(scope), args.reason, duration],
    );
    await tx(
      `UPDATE platform.support_tickets
          SET status = 'access_requested', updated_at = now()
        WHERE id = $1 AND status IN ('new', 'triaged', 'waiting_for_customer')`,
      [args.ticketId],
    );
    await enqueueBridgeEvent(tx, {
      eventType: 'support_session_requested',
      aggregateType: 'support_session',
      aggregateId: result.rows[0].id,
      aggregateVersion: 1,
      idempotencyKey: `folio-support-session-requested-${result.rows[0].id}`,
      payload: {
        sessionId: result.rows[0].id,
        ticketId: args.ticketId,
        customerId: args.customerId,
        agentId: args.agentId,
        mode: args.mode,
        permissions: args.permissions ?? [],
        scope,
        requestedDurationMinutes: duration,
        status: result.rows[0].status,
        expiresAt: result.rows[0].expires_at,
      },
    });
    return result.rows[0];
  });
}

export async function revokeSupportSession(id: string, revokedBy: string): Promise<void> {
  await withTransaction(async (tx) => {
    const result = await tx<{ customer_account_id: string; ticket_id: string; status: string }>(
      `UPDATE platform.support_sessions
          SET revoked_at = now(), status = 'revoked'
        WHERE id = $1 AND revoked_at IS NULL AND status IN ('requested', 'approved', 'active')
        RETURNING customer_account_id::text, ticket_id::text, status`,
      [id],
    );
    const session = result.rows[0];
    if (!session) throw new Error('support session is no longer active');
    await tx(
      `INSERT INTO platform.support_access_audit
        (session_id, ticket_id, customer_account_id, action, purpose, correlation_id)
       VALUES ($1, $2, $3, 'session_revoked', $4, $5)`,
      [id, session.ticket_id, session.customer_account_id, `Revoked by ${revokedBy}`, randomUUID()],
    );
    await enqueueBridgeEvent(tx, {
      eventType: 'support_session_revoked',
      aggregateType: 'support_session',
      aggregateId: id,
      aggregateVersion: session.status === 'requested' ? 2 : session.status === 'approved' ? 3 : 4,
      idempotencyKey: `folio-support-session-revoked-${id}`,
      payload: { sessionId: id, ticketId: session.ticket_id, customerId: session.customer_account_id, revokedBy },
    });
  });
}

export async function approveSupportSession(args: {
  sessionId: string;
  customerId: string;
  approver: string;
}): Promise<{ id: string; status: string; expires_at: string }> {
  return withTransaction(async (tx) => {
    const current = await tx<{ customer_account_id: string; ticket_id: string; expires_at: string; revoked_at: string | null; status: string }>(
      `SELECT customer_account_id::text, ticket_id::text, expires_at::text, revoked_at::text, status
         FROM platform.support_sessions
        WHERE id = $1
        FOR UPDATE`,
      [args.sessionId],
    );
    const session = current.rows[0];
    if (!session) throw new Error('support session not found');
    if (session.customer_account_id !== args.customerId) throw new Error('customer approval scope mismatch');
    if (session.revoked_at || new Date(session.expires_at).getTime() <= Date.now()) throw new Error('support session is expired or revoked');
    if (!['requested', 'approved'].includes(session.status)) throw new Error('support session is not awaiting approval');
    const result = await tx<{ id: string; status: string; expires_at: string }>(
      `UPDATE platform.support_sessions
          SET approved_by = $2, approved_at = now(), status = 'approved'
        WHERE id = $1
        RETURNING id::text, status, expires_at::text`,
      [args.sessionId, args.approver],
    );
    await tx(
      `UPDATE platform.support_tickets
          SET status = 'approved', updated_at = now()
        WHERE id = $1 AND status = 'access_requested'`,
      [session.ticket_id],
    );
    await tx(
      `INSERT INTO platform.support_access_audit
        (session_id, ticket_id, customer_account_id, action, purpose, correlation_id)
       VALUES ($1, $2, $3, 'session_approved', 'customer_approval', $4)`,
      [args.sessionId, session.ticket_id, args.customerId, randomUUID()],
    );
    await enqueueBridgeEvent(tx, {
      eventType: 'support_session_approved',
      aggregateType: 'support_session',
      aggregateId: args.sessionId,
      aggregateVersion: 2,
      idempotencyKey: `folio-support-session-approved-${args.sessionId}`,
      payload: { sessionId: args.sessionId, ticketId: session.ticket_id, customerId: args.customerId, approvedBy: args.approver, status: result.rows[0].status, expiresAt: result.rows[0].expires_at },
    });
    return result.rows[0];
  });
}

export async function authorizeSupportSessionAction(args: {
  sessionId: string;
  agentId: number;
  permission: string;
  moduleKey?: string | null;
  companyId?: string | null;
  branchId?: string | null;
  purpose: string;
}): Promise<{ customerId: string; mode: string; representedTenantUser: string | null; correlationId: string }> {
  return withTransaction(async (tx) => {
    const result = await tx<{ customer_account_id: string; ticket_id: string; agent_user_id: number | null; mode: string; permissions: string[]; scope: Record<string, unknown>; represented_tenant_user: string | null }>(
      `SELECT ss.customer_account_id::text, ss.ticket_id::text, ss.agent_user_id, ss.mode,
              ss.permissions, ss.scope, ss.represented_tenant_user
         FROM platform.support_sessions ss
         JOIN platform.support_tickets st ON st.id = ss.ticket_id
        WHERE ss.id = $1
          AND ss.agent_user_id = $2
          AND ss.status IN ('approved', 'active')
          AND ss.revoked_at IS NULL
          AND ss.expires_at > now()
          AND st.status NOT IN ('closed', 'customer_confirmed')
        FOR UPDATE`,
      [args.sessionId, args.agentId],
    );
    const session = result.rows[0];
    if (!session) throw new Error('support capability is not active');
    if (!session.permissions.includes(args.permission)) throw new Error('support capability does not grant the requested permission');
    if ((args.companyId && !isUuid(args.companyId)) || (args.branchId && !isUuid(args.branchId))) {
      throw new Error('support action scope identifiers must be UUIDs');
    }
    if (args.companyId || args.branchId) {
      const target = await tx<{ allowed: boolean }>(
        `SELECT EXISTS(
           SELECT 1 FROM platform.companies co
           JOIN platform.holdings h ON h.id = co.holding_id
          WHERE h.customer_account_id = $1
            AND ($2::uuid IS NULL OR co.id = $2)
            AND ($3::uuid IS NULL OR EXISTS(
              SELECT 1 FROM platform.branches b WHERE b.id = $3 AND b.company_id = co.id
            ))
        ) AS allowed`,
        [session.customer_account_id, args.companyId ?? null, args.branchId ?? null],
      );
      if (!target.rows[0]?.allowed) throw new Error('support action is outside the customer scope');
    }
    const modules = Array.isArray(session.scope?.modules) ? session.scope.modules : [];
    if (args.moduleKey && modules.length > 0 && !modules.includes(args.moduleKey)) throw new Error('support capability does not cover the requested module');
    const scopedCompanyId = typeof session.scope?.companyId === 'string' ? session.scope.companyId : null;
    const scopedBranchId = typeof session.scope?.branchId === 'string' ? session.scope.branchId : null;
    if (scopedBranchId && args.branchId !== scopedBranchId) throw new Error('support action is outside the approved branch scope');
    if (scopedCompanyId && args.companyId && args.companyId !== scopedCompanyId) throw new Error('support action is outside the approved company scope');
    if (scopedCompanyId && args.branchId) {
      const branch = await tx<{ allowed: boolean }>(
        `SELECT EXISTS(
           SELECT 1 FROM platform.branches b
           JOIN platform.companies co ON co.id = b.company_id
          WHERE b.id = $1 AND co.id = $2
        ) AS allowed`,
        [args.branchId, scopedCompanyId],
      );
      if (!branch.rows[0]?.allowed) throw new Error('support action is outside the approved company scope');
    }
    const correlationId = randomUUID();
    await tx(
      `UPDATE platform.support_sessions SET status = 'active' WHERE id = $1 AND status = 'approved'`,
      [args.sessionId],
    );
    await tx(
      `INSERT INTO platform.support_access_audit
        (session_id, ticket_id, customer_account_id, platform_actor_id, represented_tenant_user, action, module_key, company_id, branch_id, purpose, correlation_id)
       VALUES ($1, $2, $3, $4, $5, 'support_action', $6, $7, $8, $9, $10)`,
      [args.sessionId, session.ticket_id, session.customer_account_id, args.agentId, session.represented_tenant_user, args.moduleKey ?? null, args.companyId ?? null, args.branchId ?? null, args.purpose, correlationId],
    );
    return { customerId: session.customer_account_id, mode: session.mode, representedTenantUser: session.represented_tenant_user, correlationId };
  });
}
