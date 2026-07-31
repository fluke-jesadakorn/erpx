import 'server-only';

import { createHmac, timingSafeEqual } from 'node:crypto';

export function verifyCustomerApprovalToken(args: { token: string; sessionId: string; customerId: string }): boolean {
  const secret = process.env.ERPX_SUPPORT_APPROVAL_SECRET;
  if (!secret || secret.length < 32 || !args.token) return false;
  const expected = createHmac('sha256', secret).update(`${args.sessionId}:${args.customerId}`).digest('hex');
  const supplied = Buffer.from(args.token, 'utf8');
  const target = Buffer.from(expected, 'utf8');
  return supplied.length === target.length && timingSafeEqual(supplied, target);
}
