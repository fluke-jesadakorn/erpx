import { hospitalityOwnerProjectionSources as billingProjectionSources } from "../../billing/public/hospitality-owner-projection-sources";
import type {PoolClient,Pool} from 'pg';
export function billingHoldingFacts(db:Pick<PoolClient|Pool,'query'>){return {
 holdingExists:(id:string)=>db.query('SELECT 1 FROM holding_bindings WHERE holding_id=$1',[id]),
 holdingSlug:(id:string)=>db.query<{slug:string}>('SELECT slug FROM holding_bindings WHERE holding_id=$1',[id]),
 platformAccounts:()=>db.query<Record<string,unknown>>(`SELECT h.holding_id,h.slug,h.display_name,a.version,a.payment_mode FROM holding_bindings h LEFT JOIN (${billingProjectionSources.billing_accounts}) a ON a.holding_id=h.holding_id WHERE h.holding_id <> 'h003' ORDER BY h.display_name`),
}}
