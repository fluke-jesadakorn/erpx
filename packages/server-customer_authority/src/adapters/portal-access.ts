import "server-only";
import type { SqlDatabase, SqlStatement } from "@erpx/server-runtime/public/runtime/types";
import type { CustomerPortalSessionActor, CustomerPortalBindingScope, PortalBinding } from "@erpx/server-customer_360/public/portal-contract";
import {customerPortalPurchaseScope} from '@erpx/server-customer_authority/adapters/portal-read-purchases';
import {portalBindingSource} from './portal-binding-source';
import {customerAuthorityWave3Statement} from './native-customer-authority-wave3';
import {existenceProbe} from './customer-authority-read-tail-r30-sources';
import {CA_RD_PORTAL_BINDING_LOCK,nativePortalReadStatement} from './native-owner-read';
export async function projectCustomerPortalAccess(db: SqlDatabase, actor: {userId:string;email:string}, now:number, readId:string) {
  const purchase=await customerPortalPurchaseScope(db,'b');
  return (await (await customerAuthorityWave3Statement(db,{kind:'system'},'portal_access_audit',[actor.userId,actor.email.toLowerCase(),actor.email.toLowerCase(),now,readId,actor.userId,now,actor.userId,purchase],`portal-access:${readId}`,now)).all<Record<string,unknown>>()).results;
}
export async function lockCustomerPortalBinding(db: SqlDatabase, id:string, email:string) {
  const plan=await nativePortalReadStatement(CA_RD_PORTAL_BINDING_LOCK,[id,email]);
  return db.prepare(plan.sql).bind(...plan.values).first<PortalBinding>();
}
export async function customerPortalIdentityExists(db:SqlDatabase, bindingId:string,actor:CustomerPortalSessionActor) {
  return !!await db.prepare(existenceProbe('FROM customer_access_identities','binding_id=? AND user_id=? AND email=?')).bind(bindingId,actor.userId,actor.email.toLowerCase()).first();
}
export async function readCustomerPortalBindingScope(db:SqlDatabase,actor:Pick<CustomerPortalSessionActor,"userId"|"email">,bindingId:string,now:number) {
  const purchase=await customerPortalPurchaseScope(db,'b');
  return db.prepare(`${portalBindingSource('b.id,')}
    JOIN customer_access_identities i ON i.binding_id=b.id AND i.user_id=? AND i.email=?
    WHERE b.id=? AND b.email=? AND b.status='approved' AND b.expires_at>? AND ${purchase}
      AND o.kind IN ('sales','quote') AND o.state IN ('confirmed','converted','partially_fulfilled','fulfilled')`).bind(actor.userId,actor.email.toLowerCase(),bindingId,actor.email.toLowerCase(),now).first<CustomerPortalBindingScope>();
}
export async function prepareCustomerPortalEvidenceChange(db:SqlDatabase,input:{now:number;bindingId:string;companyId:string;version:number;actorId:string;key:string}):Promise<SqlStatement[]> {
  const scope={kind:'company' as const,id:input.companyId};
  return [
    await customerAuthorityWave3Statement(db,scope,'portal_evidence_binding_version',[input.now,input.bindingId,input.companyId,input.version],`portal-evidence-version:${input.key}`,input.now),
    await customerAuthorityWave3Statement(db,scope,'portal_evidence_event',[`customer-access-event-${input.key}`,input.bindingId,input.version+1,input.actorId,'Customer attached clean evidence to the support request',input.now],`portal-evidence-event:${input.key}`,input.now),
  ];
}
