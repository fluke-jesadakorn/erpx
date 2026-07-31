import { NextResponse } from 'next/server';
import { controlReadQuery } from '@/db';
import { platformApiGuard, PLATFORM_SUPPORT } from '@/platform/access';
import { boundedOptionalText, boundedStringArray, boundedText, isUuid, objectBody, safeApiError } from '@/platform/input';
import { requestSupportSession } from '@/platform/queries';
import type { SupportMode } from '@/platform/types';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

const MODES = new Set<SupportMode>(['diagnostic_read', 'tenant_mutation', 'impersonation']);

function mapSession(row: Record<string, unknown>) {
  return {
    id: String(row.id),
    ticket_id: String(row.ticket_id),
    customer_account_id: String(row.customer_account_id),
    agent_name: String(row.agent_name),
    represented_tenant_user: row.represented_tenant_user == null ? null : String(row.represented_tenant_user),
    mode: String(row.mode),
    permissions: Array.isArray(row.permissions) ? row.permissions.map(String) : [],
    scope: row.scope && typeof row.scope === 'object' && !Array.isArray(row.scope) ? row.scope : {},
    reason: String(row.reason),
    requested_duration_minutes: Number(row.requested_duration_minutes ?? 0),
    approved_by: row.approved_by == null ? null : String(row.approved_by),
    approved_at: row.approved_at == null ? null : String(row.approved_at),
    requested_at: String(row.requested_at),
    expires_at: String(row.expires_at),
    revoked_at: row.revoked_at == null ? null : String(row.revoked_at),
    status: String(row.status),
  };
}

export async function GET(req: Request) {
  const guard = await platformApiGuard(req, PLATFORM_SUPPORT);
  if (guard.response) return guard.response;
  const customerId = new URL(req.url).searchParams.get('customerId')?.trim() ?? '';
  if (!isUuid(customerId)) return NextResponse.json({ ok: false, error: 'invalid customer id' }, { status: 400 });
  const result = await controlReadQuery<Record<string, unknown>>(
    `SELECT id::text, ticket_id::text, customer_account_id::text, agent_name,
            represented_tenant_user, mode, permissions, scope, reason,
            requested_duration_minutes, approved_by, approved_at::text,
            requested_at::text, expires_at::text, revoked_at::text, status
       FROM platform.support_sessions
      WHERE customer_account_id = $1
      ORDER BY requested_at DESC
      LIMIT 60`,
    [customerId],
  );
  return NextResponse.json({ ok: true, sessions: result.rows.map(mapSession) }, { headers: { 'cache-control': 'no-store, max-age=0' } });
}

export async function POST(req: Request) {
  const guard = await platformApiGuard(req, PLATFORM_SUPPORT);
  if (guard.response) return guard.response;
  const body = objectBody(await req.json().catch(() => null));
  const mode = String(body.mode ?? 'diagnostic_read') as SupportMode;
  const ticketId = String(body.ticketId ?? '').trim();
  const customerId = String(body.customerId ?? '').trim();
  const reason = boundedText(body.reason, 2000) ?? '';
  const durationMinutes = Number(body.durationMinutes ?? (mode === 'diagnostic_read' ? 60 : 30));
  const requestedScope = objectBody(body.scope);
  if (!MODES.has(mode) || !isUuid(ticketId) || !isUuid(customerId) || reason.length < 10 || !Number.isFinite(durationMinutes) || durationMinutes < 1) {
    return NextResponse.json({ ok: false, error: 'ticket, customer, mode, duration, and a reason are required' }, { status: 400 });
  }
  try {
    const session = await requestSupportSession({
      ticketId,
      customerId,
      agentId: guard.actor.id,
      agentName: `${guard.actor.fullname} (${guard.actor.employee_code})`,
      representedTenantUser: boundedOptionalText(body.representedTenantUser, 200),
      mode,
      permissions: boundedStringArray(body.permissions, 40, 200),
      scope: { ticket: ticketId, modules: boundedStringArray(requestedScope.modules ?? body.modules, 20, 120) },
      reason,
      durationMinutes: Math.floor(durationMinutes),
    });
    return NextResponse.json({ ok: true, session }, { status: 201 });
  } catch (error) {
    return NextResponse.json({ ok: false, error: safeApiError(error, 'support_session_failed') }, { status: 400 });
  }
}
