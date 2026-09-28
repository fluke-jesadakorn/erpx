import "server-only";

import { currentSession, type WorkContext } from "./authorization";
import { ApiError } from "./http";
import { holdingBindingBySlug, holdingResources, type HoldingId } from "./runtime";
import type { FileStore, SqlDatabase } from "./runtime/types";

export type CustomerPortalSessionActor = {
  userId: string;
  email: string;
  holdingId?: HoldingId;
  holdingSlug?: string;
  holdingName?: string;
};

export type CustomerPortalBindingScope = {
  id: string;
  company_id: string;
  branch_id: string | null;
  department_id: string | null;
  version: number;
};

export async function customerPortalBindingScope(
  db: SqlDatabase,
  actor: Pick<CustomerPortalSessionActor, "userId" | "email">,
  bindingId: string,
) {
  const row = await db.prepare(`SELECT b.id,b.company_id,o.branch_id,o.department_id,b.version
    FROM customer_access_bindings b
    JOIN commerce_orders o ON o.id=b.order_id AND o.company_id=b.company_id AND o.party_id=b.customer_id
    JOIN customer_access_identities i ON i.binding_id=b.id AND i.user_id=? AND i.email=?
    WHERE b.id=? AND b.email=? AND b.status='approved' AND b.expires_at>?
      AND o.kind IN ('sales','quote') AND o.state IN ('confirmed','converted','partially_fulfilled','fulfilled')`)
    .bind(actor.userId, actor.email.toLowerCase(), bindingId, actor.email.toLowerCase(), Date.now())
    .first<CustomerPortalBindingScope>();
  if (!row)
    throw new ApiError(404, "CUSTOMER_PORTAL_ATTACHMENT_UNAVAILABLE", "This customer file area is unavailable.");
  return row;
}

export function customerPortalWorkContext(
  session: Required<CustomerPortalSessionActor>,
  binding: CustomerPortalBindingScope,
): WorkContext {
  return {
    userId: session.userId,
    name: session.email,
    email: session.email.toLowerCase(),
    assignmentId: `customer-portal:${binding.id}`,
    role: "external_customer",
    companyId: binding.company_id,
    branchId: null,
    departmentId: null,
    holdingId: session.holdingId,
    holdingSlug: session.holdingSlug,
    holdingName: session.holdingName,
    scope: {
      kind: "holding",
      holdingId: session.holdingId,
      holdingSlug: session.holdingSlug,
      holdingName: session.holdingName,
      companyId: binding.company_id,
    },
    assignmentScope: "holding",
    inherited: false,
    modules: [],
    assignments: [],
    workspaces: [],
  };
}

export async function customerPortalAttachmentSession(holdingSlug: string) {
  const { session } = await currentSession();
  if (!session.user.emailVerified)
    throw new ApiError(403, "CUSTOMER_EMAIL_UNVERIFIED", "Verify your account email before opening customer documents.");
  const holding = await holdingBindingBySlug(holdingSlug);
  const resources = await holdingResources(holding.id);
  const actor: Required<CustomerPortalSessionActor> = {
    userId: session.user.id,
    email: session.user.email.toLowerCase(),
    holdingId: holding.id,
    holdingSlug: holding.slug,
    holdingName: holding.displayName,
  };
  const context = { ...resources, actor };
  return context;
}

export async function customerPortalAttachmentContext(holdingSlug: string, bindingId: string) {
  const context = await customerPortalAttachmentSession(holdingSlug);
  const binding = await customerPortalBindingScope(context.write, context.actor, bindingId);
  return {
    ...context,
    binding,
    attachmentActor: customerPortalWorkContext(context.actor as Required<CustomerPortalSessionActor>, binding),
  };
}

export async function customerPortalAttachmentTarget(
  db: SqlDatabase,
  kind: "set" | "link" | "upload",
  id: string,
) {
  const predicate = kind === "set"
    ? "s.id=?"
    : kind === "link"
      ? "l.id=?"
      : "u.id=?";
  const joins = kind === "set"
    ? ""
    : kind === "link"
      ? "JOIN attachment_links l ON l.set_id=s.id"
      : "JOIN upload_intents u ON u.attachment_set_id=s.id";
  const row = await db.prepare(`SELECT s.target_id,s.target_type,s.purpose_key
    FROM attachment_sets s ${joins}
    WHERE ${predicate} AND s.target_type='customer_access_binding'
      AND s.purpose_key='customer_support_evidence'`)
    .bind(id)
    .first<{ target_id: string; target_type: string; purpose_key: string }>();
  if (!row)
    throw new ApiError(404, "CUSTOMER_PORTAL_ATTACHMENT_UNAVAILABLE", "This customer file is unavailable.");
  return row.target_id;
}

export type CustomerSharedFile = {
  linkId: string;
  label: string;
  previewAvailable: true;
};

export async function customerPortalSharedFiles(
  db: SqlDatabase,
  actor: Pick<CustomerPortalSessionActor, "userId" | "email">,
  bindingIds: string[],
) {
  if (!bindingIds.length) return new Map<string, CustomerSharedFile[]>();
  const rows = (await db.prepare(`SELECT b.id AS binding_id,l.id AS link_id,
      COALESCE(NULLIF(l.caption,''),'Shared support document') AS label
    FROM customer_access_bindings b
    JOIN customer_access_identities i ON i.binding_id=b.id AND i.user_id=? AND i.email=?
    JOIN commerce_orders o ON o.id=b.order_id AND o.company_id=b.company_id AND o.party_id=b.customer_id
      AND o.kind IN ('sales','quote') AND o.state IN ('confirmed','converted','partially_fulfilled','fulfilled')
    JOIN support_tickets t ON t.order_id=b.order_id AND t.company_id=b.company_id AND t.source_kind='sales_order'
    JOIN attachment_sets s ON s.company_id=t.company_id AND s.target_type='support_ticket'
      AND s.target_id=t.id AND s.purpose_key='customer_shareable' AND s.state='bound'
    JOIN attachment_links l ON l.set_id=s.id AND l.state='bound'
    JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id
      AND a.source_state='available' AND a.preview_object_key IS NOT NULL
    WHERE b.id IN (${bindingIds.map(() => "?").join(",")}) AND b.email=?
      AND b.status='approved' AND b.expires_at>?
    ORDER BY b.id,l.bound_at,l.id`)
    .bind(actor.userId, actor.email.toLowerCase(), ...bindingIds, actor.email.toLowerCase(), Date.now())
    .all<{ binding_id: string; link_id: string; label: string }>()).results;
  const result = new Map<string, CustomerSharedFile[]>();
  for (const row of rows) {
    const files = result.get(row.binding_id) ?? [];
    files.push({ linkId: row.link_id, label: row.label, previewAvailable: true });
    result.set(row.binding_id, files);
  }
  return result;
}

export async function customerPortalSharedPreview(
  db: SqlDatabase,
  files: FileStore,
  actor: Pick<CustomerPortalSessionActor, "userId" | "email">,
  linkId: string,
) {
  const row = await db.prepare(`SELECT s.company_id,l.id AS link_id,a.id AS attachment_id,
      a.preview_object_key,a.preview_media_type
    FROM attachment_links l
    JOIN attachment_sets s ON s.id=l.set_id AND s.target_type='support_ticket'
      AND s.purpose_key='customer_shareable' AND s.state='bound'
    JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id
      AND a.source_state='available' AND a.preview_object_key IS NOT NULL
    JOIN support_tickets t ON t.id=s.target_id AND t.company_id=s.company_id AND t.source_kind='sales_order'
    JOIN customer_access_bindings b ON b.order_id=t.order_id AND b.company_id=t.company_id
      AND b.email=? AND b.status='approved' AND b.expires_at>?
    JOIN commerce_orders o ON o.id=b.order_id AND o.company_id=b.company_id AND o.party_id=b.customer_id
      AND o.kind IN ('sales','quote') AND o.state IN ('confirmed','converted','partially_fulfilled','fulfilled')
    JOIN customer_access_identities i ON i.binding_id=b.id AND i.user_id=? AND i.email=?
    WHERE l.id=? AND l.state='bound'`)
    .bind(actor.email.toLowerCase(), Date.now(), actor.userId, actor.email.toLowerCase(), linkId)
    .first<{ company_id: string; link_id: string; attachment_id: string; preview_object_key: string; preview_media_type: string }>();
  if (!row)
    throw new ApiError(404, "CUSTOMER_SHARED_FILE_NOT_FOUND", "Shared file not found.");
  const object = await files.get(row.preview_object_key);
  if (!object) throw new ApiError(503, "FILE_UNAVAILABLE", "The shared preview is temporarily unavailable.");
  await db.prepare(`INSERT INTO attachment_access_events(
    id,company_id,attachment_id,link_id,actor_id,variant,outcome,created_at
  ) VALUES(?,?,?,?,?,'preview','allowed',?)`)
    .bind(`portal-attachment-access-${crypto.randomUUID()}`, row.company_id, row.attachment_id, row.link_id, actor.userId, Date.now())
    .run();
  return { object, mediaType: row.preview_media_type };
}
