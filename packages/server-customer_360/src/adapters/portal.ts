import "server-only";
import {createHash} from 'node:crypto';
import type { SqlDatabase, TransactionalSqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import { ApiError } from "@erpx/server-runtime/public/http";
import { hash } from "@erpx/server-legal/public/legal/service";
import { projectCustomerPortalAccess, lockCustomerPortalBinding, customerPortalIdentityExists, prepareCustomerPortalEvidenceChange } from "@erpx/server-customer_authority/public/portal-access";
import { prepareAttachmentBinding } from "@erpx/server-files/public/attachments/service";
import { notifyCustomerPortalStaff } from "@erpx/server-collaboration/public/customer-portal-notifications";
import { auditCustomerPortalCommand, enqueueCustomerPortalCommand, clearCustomerPortalAssertions } from "@erpx/server-workflow/public/customer-portal-events";
import { createCustomerPortalApplication, type PortalRepository } from "@erpx/server-customer_360/application/portal";
import type { CustomerPortalSessionActor, PortalActionRow } from "@erpx/server-customer_360/domain/portal";
import { customerPortalAttachmentSession, customerPortalSharedFiles } from "@erpx/server-customer_360/adapters/portal-attachments";
import {portalSupportThreadsForCustomer,portalSupportThreadsForCustomerProjection} from '@erpx/server-customer_authority/public/portal-support';
import {withCustomerPortalRequestProjection} from '@erpx/server-customer_authority/public/portal-projection-unit';
import {portalSupportInstalled} from '@erpx/server-customer_authority/public/portal-support-sources';
import {customerPortalOwnedUnit} from '@erpx/server-customer_authority/public/portal-purchases';
import {commercePackageEnforcementMode,checkPurchasedCapability,readPurchasedAccesses,purchaseCommandBoundary} from '@erpx/server-commercial/public/purchase-command-boundary';
import { scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";
import {flushFinanceDeferredEffects} from '@erpx/server-finance/public/finance-transactions';

function portalRepository(db: SqlDatabase,owned=false,assertRequestAuthority?:()=>Promise<void>): PortalRepository {
  const action = (predicate: string, values: string[]) => db.prepare(`SELECT id,binding_id,order_id,action,status,created_at,request_hash,user_id FROM customer_portal_actions WHERE ${predicate}`).bind(...values).first<PortalActionRow>();
  return {
    transaction: async callback => {
      if(owned)return callback(portalRepository(db,true));
      const execute=async(transaction:SqlDatabase)=>{
        await assertRequestAuthority?.();
        const result=await callback(portalRepository(transaction,true));
        if(assertRequestAuthority){await flushFinanceDeferredEffects(transaction);await assertRequestAuthority();}
        return result;
      };
      if(await commercePackageEnforcementMode()!=='disabled')return customerPortalOwnedUnit(db,execute);
      const transactional = db as Partial<TransactionalSqlDatabase>;
      if (!transactional.transaction) throw new ApiError(503, "CUSTOMER_PORTAL_TRANSACTION_REQUIRED", "Customer actions require a transaction.");
      return transactional.transaction(execute);
    },
    projection:async(actor,now,readId)=>{
      const rows=await projectCustomerPortalAccess(db,actor,now,readId),mode=await commercePackageEnforcementMode(),companies=[...new Set(rows.map(row=>String(row.portal_purchase_company_id)))],purchases=new Map(await Promise.all(companies.map(async company=>[company,(await readPurchasedAccesses(db,mode,company,['sales.portal']))['sales.portal']] as const))),identityRevision=createHash('sha256').update(JSON.stringify([actor.userId,actor.email.toLowerCase()])).digest('hex');
      return rows.map(row=>{const {portal_purchase_company_id,...safe}=row,access=purchases.get(String(portal_purchase_company_id));return {...safe,...(mode!=='disabled'&&access?{purchase:{settleExisting:access.settleExisting,history:access.history,revision:access.revision,identityRevision}}: {})};});
    },
    sharedFiles: (actor, ids) => customerPortalSharedFiles(db, actor, ids),
    replay: key => action("request_key=?", [key]),
    existing: (id, actionName) => action("binding_id=? AND action=?", [id, actionName]),
    binding: (id, email) => lockCustomerPortalBinding(db, id, email),
    identity: (id, actor) => customerPortalIdentityExists(db, id, actor),
    authorize:async(actor,binding,purpose)=>{
      const mode=await commercePackageEnforcementMode();if(mode==='disabled')return async()=>{};
      const scope={companyId:binding.company_id,capability:'sales.portal',purpose},key=`customer-portal-purchase:${crypto.randomUUID()}`,boundary=purchaseCommandBoundary(db,mode,key,scope)!;
      const release=async()=>{if(!await db.prepare(`SELECT ${scheduledFeaturePredicate('portal_release.company_id','customer_authority')} allowed FROM (SELECT ?::text company_id) portal_release`).bind(binding.company_id).first<boolean>('allowed'))throw new ApiError(403,'FEATURE_COMING_SOON','Customer access is not released for this Company.');};
      await release();
      await checkPurchasedCapability(db,mode,scope);await db.batch(boundary.before);
      return async()=>{
        await flushFinanceDeferredEffects(db);
        const current=await lockCustomerPortalBinding(db,binding.binding_id,actor.email.toLowerCase());
        if(!current||current.company_id!==binding.company_id||current.order_id!==binding.order_id||current.customer_id!==binding.customer_id||current.email!==binding.email||current.order_version!==binding.order_version||current.state!==binding.state||current.status!=='approved'||Number(current.expires_at)<=Date.now()||!await customerPortalIdentityExists(db,binding.binding_id,actor))throw new ApiError(403,'CUSTOMER_PORTAL_BINDING_UNAVAILABLE','The original customer binding or document changed before commit.');
        await release();
        const observed=await db.batch([...boundary.after,db.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(key)]);boundary.observe?.(observed);
      };
    },
    evidence: async (command, actor) => {
      const { input, binding, key, id, now } = command;
      const prepared = await prepareAttachmentBinding(db, actor, { attachmentSetIds: input.attachmentSetIds, bindingContract: { targetType: "customer_access_binding", bindingType: "customer_portal_support_request", contexts: [{ purposeKey: "customer_support_evidence", componentKind: "action", componentRef: "request_support" }] }, targetId: binding.binding_id, expectedTargetVersion: Number(binding.binding_version), commandId: key, bindingId: id });
      return { manifest: prepared.manifest, commit: async () => { await db.batch([...prepared.guards, ...prepareCustomerPortalEvidenceChange(db, { now, bindingId: binding.binding_id, companyId: binding.company_id, version: Number(binding.binding_version), actorId: actor.userId, key }), ...prepared.steps, clearCustomerPortalAssertions(db, key)]); } };
    },
    record: async (command, evidence) => {
      const { id, input, binding, actor, key, requestHash, now } = command;
      await db.prepare(`INSERT INTO customer_portal_actions(id,company_id,binding_id,order_id,customer_id,user_id,action,note,source_version,request_key,request_hash,status,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,'recorded',?)`).bind(id, binding.company_id, binding.binding_id, binding.order_id, binding.customer_id, actor.userId, input.action, input.note, Number(binding.order_version), key, requestHash, now).run();
      if (evidence) await evidence.commit();
    },
    notify: (command, message) => notifyCustomerPortalStaff(db, { id: command.id, message, now: command.now, orderId: command.binding.order_id, companyId: command.binding.company_id }),
    audit: (command, eventId) => auditCustomerPortalCommand(db, command, eventId),
    outbox: (command, eventId, manifest) => enqueueCustomerPortalCommand(db, command, eventId, manifest),
    uniqueConflict: error => (error as { code?: string })?.code === "23505",
  };
}
function portalApplication(db: SqlDatabase,assertRequestAuthority?:()=>Promise<void>) { return createCustomerPortalApplication({ repository: portalRepository(db,false,assertRequestAuthority), clock: { now: () => Date.now() }, identifiers: { next: () => crypto.randomUUID() }, hash, error: (status, code, message) => new ApiError(status, code, message) }); }
export const customerPortalContext = customerPortalAttachmentSession;
export async function customerPortalProjection(db: SqlDatabase, actor: { userId: string; email: string }) { const data=await customerPortalOwnedUnit(db,tx=>portalApplication(tx).customerPortalProjection(actor));if(!await portalSupportInstalled(db))return data;return {...data,items:await Promise.all(data.items.map(async item=>({...item,supportThreads:await portalSupportThreadsForCustomer(db,actor,String(item.binding_id))})))}; }
export function customerPortalRequestProjection(db:SqlDatabase,actor:CustomerPortalSessionActor,assertCurrent:()=>Promise<void>){return withCustomerPortalRequestProjection(db,actor,async database=>{const data=await portalApplication(database).customerPortalProjection(actor);return {...data,items:await Promise.all(data.items.map(async item=>({...item,supportThreads:await portalSupportThreadsForCustomerProjection(database,actor,item.binding_id)})))};},assertCurrent);}
export async function customerPortalList(slug: string) { const { write, actor } = await customerPortalContext(slug); return customerPortalProjection(write, actor); }
export function customerPortalCommand(db: PostgresDatabase, actor: CustomerPortalSessionActor, raw: unknown, key: string,assertRequestAuthority?:()=>Promise<void>) { return portalApplication(db,assertRequestAuthority).customerPortalCommand(actor, raw, key); }
