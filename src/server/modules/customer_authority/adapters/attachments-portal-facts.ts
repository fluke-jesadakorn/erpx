import { finalRootOwnerProjectionSources as commerceProjectionSources } from "../../commerce/public/final-root-owner-projection-sources";
import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {assertion} from "../../workflow/public/commands";
import type {PortalBindingRow} from "../../files/public/contracts/attachments-portal";
import {customerPortalPurchaseScope} from './portal-read-purchases';
export function portalAttachmentCustomerAuthorityFacts(db:SqlDatabase){return {
readPortalBinding:async(facts:{external:boolean;values:Array<string|number>})=>db.prepare(`SELECT b.company_id,o.branch_id,o.department_id,b.version
    FROM customer_access_bindings b
    JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=b.order_id AND o.company_id=b.company_id AND o.party_id=b.customer_id
    ${facts.external ? "JOIN customer_access_identities i ON i.binding_id=b.id AND i.user_id=? AND i.email=?" : ""}
    WHERE b.id=? AND b.company_id=? ${facts.external ? "AND b.status='approved' AND b.expires_at>?" : ""} AND ${await customerPortalPurchaseScope(db,'b')}
      AND o.kind IN ('sales','quote') ${facts.external ? "AND o.state IN ('confirmed','converted','partially_fulfilled','fulfilled')" : ""}`).bind(...(facts.external?facts.values:facts.values.slice(0,2))).first<PortalBindingRow>(),
versionGuard:(commandId:string,targetType:string,targetId:string,companyId:string,version:number,index:number)=>targetType==="customer_access_binding"?assertion(db,commandId,`attachment-owner-${index}`,"EXISTS(SELECT 1 FROM customer_access_bindings WHERE id=? AND company_id=? AND version=? AND status='approved' AND expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint)",[targetId,companyId,version]):null
};}
