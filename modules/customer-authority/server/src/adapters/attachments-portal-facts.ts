import "server-only";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import type {PortalBindingRow} from "@erpx/files-server/public/contracts/attachments-portal";
import {customerPortalPurchaseScope} from '@erpx/customer-authority-server/adapters/portal-read-purchases';
import {portalBindingSource} from './portal-binding-source';
import {existenceProbe} from './customer-authority-read-tail-r30-sources';
export function portalAttachmentCustomerAuthorityFacts(db:SqlDatabase){return {
readPortalBinding:async(facts:{external:boolean;values:Array<string|number>})=>db.prepare(`${portalBindingSource()}
    ${facts.external ? "JOIN customer_access_identities i ON i.binding_id=b.id AND i.user_id=? AND i.email=?" : ""}
    WHERE b.id=? AND b.company_id=? ${facts.external ? "AND b.status='approved' AND b.expires_at>?" : ""} AND ${await customerPortalPurchaseScope(db,'b')}
      AND o.kind IN ('sales','quote') ${facts.external ? "AND o.state IN ('confirmed','converted','partially_fulfilled','fulfilled')" : ""}`).bind(...(facts.external?facts.values:facts.values.slice(0,2))).first<PortalBindingRow>(),
versionGuard:(commandId:string,targetType:string,targetId:string,companyId:string,version:number,index:number)=>targetType==="customer_access_binding"?assertion(db,commandId,`attachment-owner-${index}`,`EXISTS(${existenceProbe('FROM customer_access_bindings',"id=? AND company_id=? AND version=? AND status='approved' AND expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint")})`,[targetId,companyId,version]):null
};}
