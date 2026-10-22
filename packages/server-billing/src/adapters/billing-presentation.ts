import 'server-only';
import { forbidden,notFound } from 'next/navigation';
import { requireManageErpxContext } from '@erpx/server-identity_access/public/authorization';
import { runtimeEnv } from '../../../../packages/server-runtime/src/public/runtime';
import { billingHoldingFacts } from '@erpx/server-runtime/public/billing-holding-facts';
import type { PostgresDatabase } from '@erpx/server-runtime/public/runtime/postgres';
import { billingView } from '@erpx/server-billing/public/billing';

export async function platformBillingPresentation(holdingId?:string) {
 const {platform}=await requireManageErpxContext();
 if(!['platform_admin','platform_auditor'].includes(platform.role))forbidden();
 const db=(await runtimeEnv()).CONTROL_DB as PostgresDatabase;
 const accounts=(await billingHoldingFacts(db.pool).platformAccounts()).rows.map(row=>({holding_id:String(row.holding_id),display_name:String(row.display_name),slug:String(row.slug)}));
 if(holdingId&&!accounts.some(account=>account.holding_id===holdingId))notFound();
 const selected=holdingId??accounts[0]?.holding_id??'';
 return {accounts,holdingId:selected,data:selected?await billingView(db,selected):null,writable:platform.role==='platform_admin'};
}
