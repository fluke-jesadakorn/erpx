import { NextResponse } from 'next/server';
import { platformApiGuard, PLATFORM_SUPPORT } from '@/platform/access';
import { boundedOptionalText, boundedText, isUuid, objectBody, safeApiError } from '@/platform/input';
import { createPlatformTicket } from '@/platform/queries';
import type { TicketSeverity } from '@/platform/types';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function POST(req: Request) {
  const guard = await platformApiGuard(req, PLATFORM_SUPPORT);
  if (guard.response) return guard.response;
  const body = objectBody(await req.json().catch(() => null));
  const customerId = String(body.customerId ?? '').trim();
  const description = boundedText(body.description, 5000) ?? '';
  if (!isUuid(customerId) || description.length < 10) {
    return NextResponse.json({ ok: false, error: 'customerId and a 10-5000 character description are required' }, { status: 400 });
  }
  const severity = String(body.severity ?? 'sev3');
  if (!['sev1', 'sev2', 'sev3', 'sev4'].includes(severity)) {
    return NextResponse.json({ ok: false, error: 'invalid severity' }, { status: 400 });
  }
  const affectedUsers = Number(body.affectedUsers ?? 0);
  if (!Number.isFinite(affectedUsers) || affectedUsers < 0 || affectedUsers > 1_000_000) {
    return NextResponse.json({ ok: false, error: 'affectedUsers must be between 0 and 1000000' }, { status: 400 });
  }
  try {
    const ticket = await createPlatformTicket({
      customerId,
      moduleKey: boundedOptionalText(body.moduleKey, 120),
      environment: body.environment === 'staging' || body.environment === 'sandbox' ? body.environment : 'production',
      severity: severity as TicketSeverity,
      category: boundedOptionalText(body.category, 80) ?? 'question',
      affectedUsers: Math.floor(affectedUsers),
      description,
      diagnosticsConsent: body.diagnosticsConsent === true,
      createdBy: `${guard.actor.fullname} (${guard.actor.employee_code})`,
    });
    return NextResponse.json({ ok: true, ticket }, { status: 201 });
  } catch (error) {
    return NextResponse.json({ ok: false, error: safeApiError(error, 'ticket_create_failed') }, { status: 400 });
  }
}
