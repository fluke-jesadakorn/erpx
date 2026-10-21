import "server-only";
import { customerPortalInvitationTokenHashMatches, verifyCustomerPortalInvitationToken } from "./portal-invitation-crypto";
import type { CustomerPortalSessionActor } from "../../customer_360/public/portal-contract";
import { holdingBindingBySlug, holdingResources, runtimeEnv } from "../../runtime/public/runtime";
import { ApiError } from "../../runtime/public/http";
import { isTransactionalSqlDatabase, type SqlDatabase } from "../../runtime/public/runtime/types";
import { commercePackageEnforcementMode, checkPurchasedCapability, companyPurchaseReadLocks } from "../../commercial/public/purchase-command-boundary";
import { commerceCompleteOwnerSourceReadLocks } from "../../commerce/public/owner-source-locks";
import { customerPortalSourceReadLocks } from "./portal-source-locks";
import { customerPortalPurchaseScope } from "./portal-read-purchases";
import { scheduledFeaturePredicate } from "../../pms/public/hospitality-rollout";
import { configuredProductHandoff } from "@/lib/runtime/product-hosts";
import type { PostgresDatabase } from "../../runtime/public/runtime/postgres";
import { customerPortalInvitableOrderStates } from "../domain/portal-invitation";

const unavailable = () => new ApiError(404,"CUSTOMER_PORTAL_INVITATION_UNAVAILABLE","This customer invitation is no longer available.");
const invitableOrderStatesSql = customerPortalInvitableOrderStates.map(state => `'${state}'`).join(",");
type Claims = NonNullable<ReturnType<typeof verifyCustomerPortalInvitationToken>>;
type Invite = { id:string; binding_id:string; holding_slug:string; generation:number; email:string; token_hash:string; token_key_version:number; token_expires_at:number; state:string; accepted_user_id:string|null; binding_status:string; binding_expires_at:number; company_id:string; order_id:string; customer_id:string; order_kind:string; order_state:string };
export type CustomerPortalEnrollmentAttemptReservation = { holdingSlug:string; bindingId:string; invitationId:string; windowStart:number };

function verifiedClaims(token: string,secret: string): Claims {
  const claims = verifyCustomerPortalInvitationToken(secret,token);
  if (!claims || Number(claims.expiresAt) <= Date.now()) throw unavailable();
  return claims;
}

async function withInvitationUnit<Result>(owner: SqlDatabase, bindingId: string, run: (database: SqlDatabase, mode: Awaited<ReturnType<typeof commercePackageEnforcementMode>>) => Promise<Result>) {
  if (!isTransactionalSqlDatabase(owner)) throw new ApiError(503,"CUSTOMER_PORTAL_TRANSACTION_REQUIRED","Customer Portal invitations require their owned Holding transaction.");
  const scope = await owner.prepare("SELECT company_id FROM customer_access_bindings WHERE id=?").bind(bindingId).first<{company_id:string}>();
  if (!scope) throw unavailable();
  const mode = await commercePackageEnforcementMode();
  return owner.transaction(async database => {
    const locks = companyPurchaseReadLocks(database,scope.company_id);
    await database.batch([locks[0],...commerceCompleteOwnerSourceReadLocks(database),...customerPortalSourceReadLocks(database)]);
    await database.batch(locks.slice(1));
    if (mode !== "disabled") await checkPurchasedCapability(database,mode,{companyId:scope.company_id,capability:"sales.portal",purpose:"settle-existing"});
    return run(database,mode);
  });
}

async function matchingInvitation(database: SqlDatabase, claims: Claims, token: string, now: number, allowAccepted: boolean): Promise<Invite | null> {
  const purchase = await customerPortalPurchaseScope(database,"binding","settle-existing");
  const row = await database.prepare(`SELECT invitation.id,invitation.binding_id,invitation.holding_slug,invitation.generation,invitation.email,invitation.token_hash,invitation.token_key_version,invitation.token_expires_at,invitation.state,invitation.accepted_user_id,
      binding.status binding_status,binding.expires_at binding_expires_at,binding.company_id,binding.order_id,binding.customer_id,
      original.kind order_kind,original.state order_state
    FROM customer_portal_invitations invitation
    JOIN customer_access_bindings binding ON binding.id=invitation.binding_id
    JOIN commerce_orders original ON original.id=binding.order_id AND original.company_id=binding.company_id AND original.party_id=binding.customer_id
    WHERE invitation.id=? AND invitation.binding_id=? AND invitation.holding_slug=? AND invitation.generation=?
      AND invitation.token_key_version=? AND invitation.token_expires_at=? AND invitation.token_expires_at>?
      AND invitation.state IN (${allowAccepted?"'sent','accepted'":"'sent'"}) AND binding.email=invitation.email AND binding.status='approved' AND binding.expires_at>?
      AND original.kind IN ('quote','sales') AND original.state IN (${invitableOrderStatesSql})
      AND ${scheduledFeaturePredicate("binding.company_id","customer_authority")} AND ${purchase}
    FOR UPDATE OF invitation,binding,original`)
    .bind(claims.invitationId,claims.bindingId,claims.holdingSlug,claims.generation,claims.keyVersion,claims.expiresAt,now,now)
    .first<Invite>();
  if (!row || !customerPortalInvitationTokenHashMatches(token,row.token_hash)) return null;
  return row;
}

export async function customerPortalInvitationEnrollment(token: string) {
  const env=await runtimeEnv(),claims = verifiedClaims(token,env.BETTER_AUTH_SECRET), holding = await holdingBindingBySlug(claims.holdingSlug), owner = (await holdingResources(holding.id)).write;
  return withInvitationUnit(owner,claims.bindingId,async database => {
    const invite = await matchingInvitation(database,claims,token,Date.now(),false);
    if (!invite) throw unavailable();
    const now=Date.now(),attempt = await database.prepare(`INSERT INTO customer_portal_enrollment_attempts(invitation_id,window_start,attempts) VALUES(?,?,1)
      ON CONFLICT(invitation_id) DO UPDATE SET attempts=CASE WHEN customer_portal_enrollment_attempts.window_start<? THEN 1 ELSE customer_portal_enrollment_attempts.attempts+1 END,
        window_start=CASE WHEN customer_portal_enrollment_attempts.window_start<? THEN excluded.window_start ELSE customer_portal_enrollment_attempts.window_start END
      RETURNING attempts,window_start`).bind(invite.id,now,now-3600000,now-3600000).first<{attempts:number;window_start:number}>();
    return { email: invite.email, attempts: Number(attempt?.attempts ?? 0), reservation:{holdingSlug:invite.holding_slug,bindingId:invite.binding_id,invitationId:invite.id,windowStart:Number(attempt?.window_start??now)} satisfies CustomerPortalEnrollmentAttemptReservation };
  });
}

/** Release only this in-flight signup reservation after an infrastructure failure. */
export async function releaseCustomerPortalEnrollmentAttempt(reservation:CustomerPortalEnrollmentAttemptReservation) {
  const holding=await holdingBindingBySlug(reservation.holdingSlug),owner=(await holdingResources(holding.id)).write;
  return withInvitationUnit(owner,reservation.bindingId,async database=>{
    await database.prepare('DELETE FROM customer_portal_enrollment_attempts WHERE invitation_id=? AND window_start=? AND attempts<=1')
      .bind(reservation.invitationId,reservation.windowStart).run();
    await database.prepare(`UPDATE customer_portal_enrollment_attempts SET attempts=attempts-1
      WHERE invitation_id=? AND window_start=? AND attempts>1`).bind(reservation.invitationId,reservation.windowStart).run();
  });
}

export async function acceptCustomerPortalInvitation(token: string, actor: Pick<CustomerPortalSessionActor,"userId"|"email">, assertCurrent: () => Promise<void>) {
  const env=await runtimeEnv(),claims = verifiedClaims(token,env.BETTER_AUTH_SECRET), holding = await holdingBindingBySlug(claims.holdingSlug), owner = (await holdingResources(holding.id)).write;
  const result = await withInvitationUnit(owner,claims.bindingId,async database => {
    await assertCurrent();
    const invite = await matchingInvitation(database,claims,token,Date.now(),true);
    if (!invite || invite.holding_slug!==holding.slug || invite.email!==actor.email.toLowerCase()) throw unavailable();
    if (invite.state === "accepted") {
      const identity = await database.prepare("SELECT user_id,email FROM customer_access_identities WHERE binding_id=?").bind(invite.binding_id).first<{user_id:string;email:string}>();
      if (invite.accepted_user_id!==actor.userId || identity?.user_id!==actor.userId || identity.email!==actor.email.toLowerCase()) throw unavailable();
      await assertCurrent();
      return { bindingId:invite.binding_id,holdingSlug:invite.holding_slug,accepted:true };
    }
    await database.prepare("INSERT INTO customer_access_identities(binding_id,user_id,email,bound_at) VALUES(?,?,?,?) ON CONFLICT(binding_id) DO NOTHING")
      .bind(invite.binding_id,actor.userId,actor.email.toLowerCase(),Date.now()).run();
    const identity = await database.prepare("SELECT user_id,email FROM customer_access_identities WHERE binding_id=? FOR UPDATE").bind(invite.binding_id).first<{user_id:string;email:string}>();
    if (identity?.user_id!==actor.userId || identity.email!==actor.email.toLowerCase()) throw unavailable();
    const now = Date.now();
    const changed = await database.prepare("UPDATE customer_portal_invitations SET state='accepted',accepted_at=?,accepted_user_id=? WHERE id=? AND state='sent' RETURNING id")
      .bind(now,actor.userId,invite.id).first<{id:string}>();
    if (!changed) throw unavailable();
    await database.prepare("INSERT INTO customer_portal_invitation_events(id,invitation_id,action,actor_id,reason,created_at) VALUES(?,?,'accepted',?,'Verified invited customer accepted this exact binding.',?)")
      .bind(`customer-portal-invitation-event-accepted:${invite.id}`,invite.id,actor.userId,now).run();
    await assertCurrent();
    return { bindingId:invite.binding_id,holdingSlug:invite.holding_slug,accepted:true };
  });
  const portalPath=`/customer-portal/${encodeURIComponent(result.holdingSlug)}`;
  return { ...result, portalUrl:configuredProductHandoff("portal",portalPath)??portalPath };
}
