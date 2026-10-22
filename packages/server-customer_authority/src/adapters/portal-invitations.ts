import "server-only";
import { randomUUID } from "node:crypto";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase, SqlStatement } from "@erpx/server-runtime/public/runtime/types";
import type { Row } from "@erpx/server-commerce/public/commerce";
import { holdingResources, runtimeEnv } from "../../../../packages/server-runtime/src/public/runtime";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { ApiError } from "@erpx/server-runtime/public/http";
import { executeCommand, replayCommand } from "@erpx/server-workflow/public/commands";
import { finalRootOwnerProjectionSources as commerce } from "@erpx/server-commerce/public/final-root-owner-projection-sources";
import { createCustomerPortalInvitationApplication, type CustomerPortalInvitationBinding, type CustomerPortalInvitationHistory, type CustomerPortalInvitationRepository, type CustomerPortalInvitationRow } from "@erpx/server-customer_authority/application/portal-invitations";
import { customerPortalInvitationCommandSchema, isCustomerPortalInvitableOrderState } from "@erpx/server-customer_authority/domain/portal-invitation";
import { customerPortalInvitationToken, customerPortalInvitationTokenHash } from "@erpx/server-customer_authority/adapters/portal-invitation-crypto";
import { customerAccessBinding } from "@erpx/server-customer_authority/adapters/access";
import { requireCustomerAccessPurchasedRead, withCustomerAccessPurchasedCommand } from "@erpx/server-customer_authority/adapters/portal-purchases";

function repository(actor: WorkContext, database: SqlDatabase): { source: CustomerPortalInvitationRepository; binding: () => CustomerPortalInvitationBinding | null } {
  let lastBinding: CustomerPortalInvitationBinding | null = null;
  const source: CustomerPortalInvitationRepository = {
    async binding(id) {
      lastBinding = await database.prepare(`SELECT b.id,b.email,b.status,o.state order_state,b.expires_at,b.version,b.created_by,
          EXISTS(SELECT 1 FROM customer_access_identities identity WHERE identity.binding_id=b.id) identity_exists
        FROM customer_access_bindings b JOIN (${commerce.commerce_orders}) o ON o.id=b.order_id AND o.company_id=b.company_id AND o.party_id=b.customer_id
        WHERE b.id=? AND b.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?)
        AND o.kind IN ('quote','sales') FOR UPDATE OF b`).bind(id,actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId).first<CustomerPortalInvitationBinding>();
      return lastBinding;
    },
    async active(id, _now) {
      return database.prepare(`SELECT id,generation,state,token_expires_at FROM customer_portal_invitations
        WHERE binding_id=? AND state IN ('queued','running','sent','accepted') ORDER BY generation DESC LIMIT 1 FOR UPDATE`).bind(id).first<CustomerPortalInvitationRow>();
    },
    async nextGeneration(id) {
      const row = await database.prepare("SELECT COALESCE(max(generation),0)+1 generation FROM customer_portal_invitations WHERE binding_id=?").bind(id).first<{generation:number}>();
      return Number(row?.generation ?? 1);
    },
    replay: ({ key, operation, bindingId, input }) => replayCommand(database,actor,key,operation,bindingId,input),
    async create(input, previous) {
      const record = {
        id: input.bindingId,
        bindingId: input.bindingId,
        invitationId: input.invitationId,
        generation: input.generation,
        version: input.expectedVersion,
        state: "queued",
        expiresAt: input.tokenExpiresAt,
      };
      const effects: SqlStatement[] = [];
      if (previous) {
        const terminal = Number(previous.token_expires_at) <= input.now ? "expired" : "superseded";
        effects.push(database.prepare("UPDATE customer_portal_invitations SET state=?,lease_token=NULL,lease_until=NULL,last_error=? WHERE id=? AND state IN ('queued','running','sent')")
          .bind(terminal,terminal.toUpperCase(),previous.id));
        effects.push(database.prepare("INSERT INTO customer_portal_invitation_events(id,invitation_id,action,actor_id,reason,created_at) VALUES(?,?,?,?,?,?)")
          .bind(`customer-portal-invite-event-${randomUUID()}`,previous.id,terminal,actor.userId,input.reason,input.now));
      }
      effects.push(database.prepare(`INSERT INTO customer_portal_invitations
        (id,binding_id,holding_slug,generation,email,token_hash,token_key_version,token_expires_at,state,attempts,next_attempt_at,created_by,created_at,command_id)
        VALUES(?,?,?,?,?,?,?,?,'queued',0,?,?,?,?)`)
        .bind(input.invitationId,input.bindingId,input.holdingSlug,input.generation,String(lastBinding?.email ?? "").toLowerCase(),input.tokenHash,input.tokenKeyVersion,input.tokenExpiresAt,input.now,actor.userId,input.now,input.key));
      effects.push(database.prepare("INSERT INTO customer_portal_invitation_events(id,invitation_id,action,actor_id,reason,created_at) VALUES(?,?,'issued',?,?,?)")
        .bind(`customer-portal-invite-event-${input.key}`,input.invitationId,actor.userId,input.reason,input.now));
      return executeCommand(database,actor,input.key,input.operation,{action:input.action,expectedVersion:input.expectedVersion,reason:input.reason},input.bindingId,input.expectedVersion,record,effects);
    },
    async history(id): Promise<CustomerPortalInvitationHistory> {
      const invitations = (await database.prepare(`SELECT id,binding_id,generation,holding_slug,email,state,token_expires_at,attempts,next_attempt_at,last_error,created_by,created_at,sent_at,accepted_at,accepted_user_id
        FROM customer_portal_invitations WHERE binding_id=? ORDER BY generation DESC`).bind(id).all<Row>()).results;
      const events = (await database.prepare(`SELECT event.id,event.invitation_id,event.action,event.actor_id,event.reason,event.created_at
        FROM customer_portal_invitation_events event JOIN customer_portal_invitations invitation ON invitation.id=event.invitation_id
        WHERE invitation.binding_id=? ORDER BY event.created_at,event.id`).bind(id).all<Row>()).results;
      return { invitations, invitationEvents: events };
    },
  };
  return { source, binding: () => lastBinding };
}

function application(actor: WorkContext, database: SqlDatabase, holdingSlug: string, secret="") {
  const repo = repository(actor,database);
  return createCustomerPortalInvitationApplication({
    actorId: actor.userId,
    holdingSlug,
    canManage: () => repo.binding()?.created_by === actor.userId || hasPermission(actor,"commerce.approve"),
    clock: { now: () => Date.now() },
    identifiers: { next: () => randomUUID() },
    token: facts => customerPortalInvitationToken(secret,facts),
    tokenHash: customerPortalInvitationTokenHash,
    repository: repo.source,
    error: (status,code,message) => new ApiError(status,code,message),
  });
}

export async function issueCustomerPortalInvitation(actor: WorkContext, bindingId: string, raw: unknown, key: string) {
  requirePermission(actor,"commerce.sales");
  requirePermission(actor,"commerce.read");
  const input = customerPortalInvitationCommandSchema.parse(raw);
  const env=await runtimeEnv();
  return withCustomerAccessPurchasedCommand(actor,{key,operation:`customer.access.${input.action}`,id:bindingId,action:input.action},database => application(actor,database,actor.holdingSlug,env.BETTER_AUTH_SECRET).issue(bindingId,input,key));
}

export async function customerPortalInvitationHistory(actor: WorkContext, bindingId: string) {
  const binding = await customerAccessBinding(actor,bindingId), { write } = await holdingResources(actor.holdingId);
  await requireCustomerAccessPurchasedRead(write,actor,bindingId);
  const history=await application(actor,write,actor.holdingSlug).history(bindingId),now=Date.now();
  const sourceUsable=binding.status==='approved'&&Number(binding.expires_at)>now&&isCustomerPortalInvitableOrderState(String(binding.order_state??""));
  return { ...history, invitations:history.invitations.map(invitation=>({...invitation,isActive:sourceUsable&&['queued','running','sent'].includes(String(invitation.state))&&Number(invitation.token_expires_at)>now})), canInvite: Boolean(binding.canInvite) };
}
