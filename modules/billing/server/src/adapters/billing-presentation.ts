import 'server-only';
import { forbidden, notFound } from 'next/navigation';
import { requireManageErpxContext } from '@erpx/identity-access-server/public/authorization';
import { runtimeEnv } from '../../../../../platform/runtime/server/src/public/runtime';
import { billingHoldingFacts } from '@erpx/platform-runtime-server/public/billing-holding-facts';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import type { PostgresDatabase } from '@erpx/platform-runtime-server/public/runtime/postgres';
import { nativeBillingWorkspace } from '@erpx/billing-server/public/native-billing-page';

export async function platformBillingPresentation(holdingId?: string) {
  const { platform } = await requireManageErpxContext();
  if (!['platform_admin', 'platform_auditor'].includes(platform.role)) forbidden();
  const db = (await runtimeEnv()).CONTROL_DB as PostgresDatabase;
  const accounts = (
    await billingHoldingFacts(db.pool).platformAccounts()
  ).rows.map((row) => ({
    holding_id: String(row.holding_id),
    display_name: String(row.display_name),
    slug: String(row.slug),
  }));
  if (holdingId && !accounts.some((account) => account.holding_id === holdingId))
    notFound();
  const selected = holdingId ?? accounts[0]?.holding_id ?? '';
  if (!selected)
    return {
      accounts,
      holdingId: selected,
      data: null,
      writable: platform.role === 'platform_admin',
    };
  // The native Billing workspace is the sole authority. `nativeBillingWorkspace`
  // fails closed when the exact private configuration is missing or the owner
  // response is missing/malformed; there is no TypeScript SQL fallback.
  const data = await nativeBillingWorkspace({
    userId: platform.userId,
    holdingId: selected,
    platform: true,
    role: platform.role,
    assignmentId: platform.assignmentId,
    expiresAt: null,
  });
  if (!data)
    throw new ApiError(503, 'NATIVE_BILLING_UNAVAILABLE', 'The native Billing view is unavailable.');
  return {
    accounts,
    holdingId: selected,
    data: data ?? null,
    writable: platform.role === 'platform_admin',
  };
}
