import 'server-only';
import { forbidden, notFound } from 'next/navigation';
import { requireManageErpxContext } from '@erpx/server-identity_access/public/authorization';
import { runtimeEnv } from '../../../../packages/server-runtime/src/public/runtime';
import { billingHoldingFacts } from '@erpx/server-runtime/public/billing-holding-facts';
import type { PostgresDatabase } from '@erpx/server-runtime/public/runtime/postgres';
import { billingView } from '@erpx/server-billing/public/billing';
import {
  nativeBillingAvailable,
  nativeBillingWorkspace,
} from '@erpx/server-billing/public/native-billing-page';

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
  const native = nativeBillingAvailable();
  const data = native
    ? await nativeBillingWorkspace({
        userId: platform.userId,
        holdingId: selected,
        platform: true,
        role: platform.role,
        assignmentId: platform.assignmentId,
        expiresAt: null,
      })
    : await billingView(db, selected);
  if (native && !data)
    throw new Error('NATIVE_BILLING_UNAVAILABLE');
  return {
    accounts,
    holdingId: selected,
    data: data ?? null,
    writable: platform.role === 'platform_admin',
  };
}
