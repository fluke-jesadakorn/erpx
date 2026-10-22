import "server-only";
import { randomUUID } from "node:crypto";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import { isTransactionalSqlDatabase } from "../../runtime/public/runtime/types";
import type { BetaEnvironment } from "../../manage_erpx/public/beta-config";
import { sendBetaEmail, type BetaEmailOptions } from "../../manage_erpx/public/beta-email";
import { publicSurfaceOrigin } from "../../identity_access/public/public-surface-entry";
import { customerPortalPurchaseScope } from "./portal-read-purchases";
import { scheduledFeaturePredicate } from "../../pms/public/hospitality-rollout";
import { createCustomerPortalInvitationDelivery, type CustomerPortalInvitationDeliveryJob } from "../application/portal-invitation-delivery";
import { customerPortalInvitationToken, customerPortalInvitationTokenHashMatches } from "./portal-invitation-crypto";
import { customerPortalInvitableOrderStates } from "../domain/portal-invitation";

const systemActor = "system:customer-portal-invitation";
const invitableOrderStatesSql = customerPortalInvitableOrderStates.map(state => `'${state}'`).join(",");
function transaction<T>(database: SqlDatabase, run: (unit: SqlDatabase) => Promise<T>) {
  if (!isTransactionalSqlDatabase(database)) throw new Error("CUSTOMER_PORTAL_INVITATION_TRANSACTION_REQUIRED");
  return database.transaction(run);
}
function invitationEvent(database: SqlDatabase, id: string, invitationId: string, action: string, reason: string, now: number) {
  return database.prepare("INSERT INTO customer_portal_invitation_events(id,invitation_id,action,actor_id,reason,created_at) VALUES(?,?,?,?,?,?)")
    .bind(`customer-portal-delivery-event-${id}`,invitationId,action,systemActor,reason,now);
}

export function drainCustomerPortalInvitations(database: SqlDatabase, env: BetaEnvironment & { BETTER_AUTH_URL?: string; BETTER_AUTH_SECRET?: string }, send: (env: BetaEnvironment & { BETTER_AUTH_URL?: string; BETTER_AUTH_SECRET?: string },to:string,subject:string,body:string,key?:string,options?:BetaEmailOptions)=>Promise<unknown> = sendBetaEmail) {
  const accountOrigin = publicSurfaceOrigin("account",env.BETTER_AUTH_URL);
  return createCustomerPortalInvitationDelivery({
    repository: {
      async recover(now) {
        await transaction(database,async unit => {
          const rows = (await unit.prepare(`UPDATE customer_portal_invitations SET
            state=CASE WHEN token_expires_at<=? THEN 'expired' WHEN attempts<6 THEN 'queued' ELSE 'failed' END,
            lease_token=NULL,lease_until=NULL,last_error=CASE WHEN token_expires_at<=? THEN 'INVITATION_EXPIRED' WHEN attempts<6 THEN 'DELIVERY_INTERRUPTED' ELSE 'DELIVERY_ATTEMPTS_EXHAUSTED' END
            WHERE (state='running' AND (lease_until<? OR token_expires_at<=?))
              OR (state IN ('queued','sent') AND token_expires_at<=?) RETURNING id,state,last_error`).bind(now,now,now,now,now).all<{id:string;state:string;last_error:string}>()).results;
          for (const row of rows) await unit.prepare("INSERT INTO customer_portal_invitation_events(id,invitation_id,action,actor_id,reason,created_at) VALUES(?,?,?,?,?,?)")
            .bind(`customer-portal-delivery-recovery-${row.id}-${now}`,row.id,row.state==='expired'?'expired':row.state==='failed'?'delivery-failed':'delivery-retried',systemActor,row.last_error,now).run();
          const unavailable = (await unit.prepare(`UPDATE customer_portal_invitations invitation
            SET state='revoked',lease_token=NULL,lease_until=NULL,last_error='SOURCE_UNAVAILABLE'
            WHERE invitation.state IN ('queued','running','sent') AND invitation.token_expires_at>?
              AND NOT EXISTS (
                SELECT 1 FROM customer_access_bindings binding
                JOIN commerce_orders original ON original.id=binding.order_id AND original.company_id=binding.company_id AND original.party_id=binding.customer_id
                WHERE binding.id=invitation.binding_id AND binding.status='approved' AND binding.expires_at>?
                  AND original.kind IN ('quote','sales') AND original.state IN (${invitableOrderStatesSql})
              )
            RETURNING id`).bind(now,now).all<{id:string}>()).results;
          for (const row of unavailable) await unit.prepare("INSERT INTO customer_portal_invitation_events(id,invitation_id,action,actor_id,reason,created_at) VALUES(?,?,?,?,?,?)")
            .bind(`customer-portal-delivery-recovery-source-${row.id}-${now}`,row.id,'revoked',systemActor,'SOURCE_UNAVAILABLE',now).run();
        });
      },
      async claim(leaseToken,now) {
        return transaction(database,async unit => {
          const purchase = await customerPortalPurchaseScope(unit,"binding","settle-existing");
          const row = await unit.prepare(`UPDATE customer_portal_invitations invitation SET state='running',attempts=attempts+1,lease_token=?,lease_until=?
            WHERE invitation.id=(SELECT candidate.id FROM customer_portal_invitations candidate
              JOIN customer_access_bindings binding ON binding.id=candidate.binding_id
              JOIN commerce_orders original ON original.id=binding.order_id AND original.company_id=binding.company_id AND original.party_id=binding.customer_id
              WHERE candidate.state='queued' AND candidate.next_attempt_at<=? AND candidate.token_expires_at>?
                AND binding.status='approved' AND binding.expires_at>? AND original.kind IN ('quote','sales')
                AND original.state IN (${invitableOrderStatesSql})
                AND ${scheduledFeaturePredicate("binding.company_id","customer_authority")} AND ${purchase}
              ORDER BY candidate.created_at,candidate.id FOR UPDATE OF candidate SKIP LOCKED LIMIT 1)
            RETURNING invitation.id,invitation.binding_id,invitation.holding_slug,invitation.generation,invitation.email,
              invitation.token_hash,invitation.token_key_version,invitation.token_expires_at,invitation.attempts,invitation.lease_token`)
            .bind(leaseToken,now+30000,now,now,now).first<CustomerPortalInvitationDeliveryJob>();
          if (row) await unit.batch([invitationEvent(unit,`${row.id}-${leaseToken}`,row.id,"delivery-started","",now)]);
          return row;
        });
      },
      async current(job,now) {
        const purchase = await customerPortalPurchaseScope(database,"binding","settle-existing");
        return Boolean(await database.prepare(`SELECT 1 FROM customer_portal_invitations invitation
          JOIN customer_access_bindings binding ON binding.id=invitation.binding_id
          JOIN commerce_orders original ON original.id=binding.order_id AND original.company_id=binding.company_id AND original.party_id=binding.customer_id
          WHERE invitation.id=? AND invitation.state='running' AND invitation.lease_token=? AND invitation.holding_slug=?
            AND invitation.token_hash=? AND invitation.email=binding.email AND invitation.token_expires_at>?
            AND binding.status='approved' AND binding.expires_at>? AND original.kind IN ('quote','sales')
            AND original.state IN (${invitableOrderStatesSql})
            AND ${scheduledFeaturePredicate("binding.company_id","customer_authority")} AND ${purchase}`)
          .bind(job.id,job.lease_token,job.holding_slug,job.token_hash,now,now).first());
      },
      async cancel(job,now,state,reason) {
        await transaction(database,async unit => {
          const rows = await unit.prepare("UPDATE customer_portal_invitations SET state=?,lease_token=NULL,lease_until=NULL,last_error=? WHERE id=? AND state='running' AND lease_token=? RETURNING id")
            .bind(state,reason,job.id,job.lease_token).all<{id:string}>();
          if (rows.results.length) await unit.batch([invitationEvent(unit,`${job.id}-${state}-${now}`,job.id,state,reason,now)]);
        });
      },
      async sent(job,now) {
        await transaction(database,async unit => {
          const rows = await unit.prepare("UPDATE customer_portal_invitations SET state='sent',sent_at=?,lease_token=NULL,lease_until=NULL,last_error=NULL WHERE id=? AND state='running' AND lease_token=? RETURNING id")
            .bind(now,job.id,job.lease_token).all<{id:string}>();
          if (rows.results.length) await unit.batch([invitationEvent(unit,`${job.id}-sent-${now}`,job.id,"delivery-sent","",now)]);
        });
      },
      async failed(job,now,state,code,nextAttempt) {
        await transaction(database,async unit => {
          const rows = await unit.prepare("UPDATE customer_portal_invitations SET state=?,next_attempt_at=?,lease_token=NULL,lease_until=NULL,last_error=? WHERE id=? AND state='running' AND lease_token=? RETURNING id")
            .bind(state,nextAttempt,code,job.id,job.lease_token).all<{id:string}>();
          if (rows.results.length) await unit.batch([invitationEvent(unit,`${job.id}-failed-${now}`,job.id,state==='failed'?"delivery-failed":"delivery-retried",code,now)]);
        });
      },
    },
    clock: { now: () => Date.now() },
    identifiers: { next: () => randomUUID() },
    token: facts => {
      if (typeof env.BETTER_AUTH_SECRET !== "string" || !env.BETTER_AUTH_SECRET) throw new Error("INVITATION_SIGNING_SECRET_UNAVAILABLE");
      return customerPortalInvitationToken(env.BETTER_AUTH_SECRET,facts);
    },
    tokenHashMatches: customerPortalInvitationTokenHashMatches,
    accountOrigin,
    send: async (to,subject,body,key) => { await send(env,to,subject,body,key,{sensitive:true}); },
  });
}
