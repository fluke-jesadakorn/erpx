import { NextResponse } from 'next/server';
import { approveSupportSession } from '@/platform/queries';
import { verifyCustomerApprovalToken } from '@/platform/approval';
import { boundedText, isUuid, objectBody, safeApiError } from '@/platform/input';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

interface RouteProps {
  params: Promise<{ id: string }>;
}

export async function POST(req: Request, { params }: RouteProps) {
  const { id } = await params;
  const body = objectBody(await req.json().catch(() => null));
  const customerId = String(body.customerId ?? '').trim();
  const approver = boundedText(body.approver, 200) ?? '';
  const token = req.headers.get('x-erpx-customer-approval') ?? String(body.approvalToken ?? '');
  if (!isUuid(id) || !isUuid(customerId) || !approver || token.length > 256 || !verifyCustomerApprovalToken({ token, sessionId: id, customerId })) {
    return NextResponse.json({ ok: false, error: 'verified customer approval is required' }, { status: 403 });
  }
  try {
    const session = await approveSupportSession({ sessionId: id, customerId, approver });
    return NextResponse.json({ ok: true, session });
  } catch (error) {
    return NextResponse.json({ ok: false, error: safeApiError(error, 'support_session_approval_failed') }, { status: 400 });
  }
}
