import 'server-only';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import {
  isTransactionalSqlDatabase,
  type SqlDatabase,
} from '@erpx/server-runtime/public/runtime/types';
import { ApiError } from '@erpx/server-runtime/public/http';

/** Stable owner scope a Customer Authority proof grants for one Files withdraw.
 * A native Files command consumes this value instead of trusting a header, a
 * request label or the external customer's submitted email. */
export type PortalAttachmentWithdrawProof = Readonly<{
  kind: 'customer-portal';
  userId: string;
  email: string;
  holdingId: string;
  companyId: string;
  assignmentId: string;
  bindingId: string;
  linkId: string;
  setId: string;
  targetType: 'customer_access_binding';
  targetId: string;
}>;

type PortalActor = Pick<
  WorkContext,
  'role' | 'email' | 'userId' | 'companyId' | 'assignmentId' | 'holdingId'
>;

const TARGET = 'customer_access_binding';

function notFound() {
  return new ApiError(404, 'ATTACHMENT_LINK_NOT_FOUND', 'File link not found.');
}

function forbidden() {
  return new ApiError(
    403,
    'CUSTOMER_PORTAL_ATTACHMENT_FORBIDDEN',
    'This customer file cannot be changed by the current sign-in.',
  );
}

function bindingChanged() {
  return new ApiError(
    409,
    'CUSTOMER_PORTAL_ATTACHMENT_CHANGED',
    'The original customer document or file authority changed.',
  );
}

function requireOwner(
  actor: PortalActor,
  row: { company_id: string; set_id: string; binding_id: string },
): string {
  if (
    actor.role !== 'external_customer' ||
    !actor.email ||
    row.company_id !== actor.companyId ||
    row.set_id.length === 0 ||
    row.binding_id.length === 0
  )
    throw bindingChanged();
  const assignmentId = `customer-portal:${row.binding_id}`;
  if (actor.assignmentId !== assignmentId) throw forbidden();
  return assignmentId;
}

/** Resolve one Files link to its customer binding. A foreign link and a missing
 * link are the same 404 so the external customer cannot probe another
 * Company's evidence. */
async function linkScope(database: SqlDatabase, companyId: string, linkId: string) {
  const row = await database
    .prepare(
      "SELECT s.id set_id,s.company_id,s.target_id binding_id FROM attachment_links l \
       JOIN attachment_sets s ON s.id=l.set_id \
       WHERE l.id=? AND s.company_id=? AND s.target_type=?",
    )
    .bind(linkId, companyId, TARGET)
    .first<{ set_id: string; company_id: string; binding_id: string }>();
  if (!row) throw notFound();
  return row;
}

async function bindingCurrent(
  database: SqlDatabase,
  proof: Pick<
    PortalAttachmentWithdrawProof,
    'bindingId' | 'companyId' | 'userId' | 'email'
  >,
) {
  const email = proof.email.toLowerCase();
  const current = await database
    .prepare(
      "SELECT 1 FROM customer_access_bindings binding \
       JOIN customer_access_identities identity ON identity.binding_id=binding.id \
       WHERE binding.id=? AND binding.company_id=? AND binding.status='approved' \
       AND binding.expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint \
       AND identity.user_id=? AND identity.email=? AND binding.email=?",
    )
    .bind(proof.bindingId, proof.companyId, proof.userId, email, email)
    .first();
  if (!current) throw bindingChanged();
}

/** Resolve a verified `external_customer` / `customer-portal:<binding>` proof to
 * the exact Company and Files owner scope a native withdraw command needs.
 * The caller owns the request transaction; this port never begins or commits
 * one and never returns a staff or another customer's binding. */
export async function portalAttachmentWithdrawProof(
  database: SqlDatabase,
  actor: WorkContext,
  linkId: string,
): Promise<PortalAttachmentWithdrawProof> {
  if (!isTransactionalSqlDatabase(database))
    throw new ApiError(
      503,
      'CUSTOMER_PORTAL_ATTACHMENT_TRANSACTION_REQUIRED',
      'A customer file command requires its fresh native transaction.',
    );
  const row = await linkScope(database, actor.companyId, linkId);
  const assignmentId = requireOwner(actor, row);
  const proof: PortalAttachmentWithdrawProof = Object.freeze({
    kind: 'customer-portal',
    userId: actor.userId,
    email: actor.email,
    holdingId: actor.holdingId,
    companyId: actor.companyId,
    assignmentId,
    bindingId: row.binding_id,
    linkId,
    setId: row.set_id,
    targetType: TARGET,
    targetId: row.binding_id,
  });
  await bindingCurrent(database, proof);
  return proof;
}

/** Revalidate the same proof and link target inside the caller's final
 * transaction. A binding that expired, moved, lost the exact identity or a link
 * that changed owner cannot reach the Files write. */
export async function requirePortalAttachmentWithdrawCurrent(
  database: SqlDatabase,
  proof: PortalAttachmentWithdrawProof,
): Promise<void> {
  const actor: PortalActor = {
    role: 'external_customer',
    email: proof.email,
    userId: proof.userId,
    companyId: proof.companyId,
    assignmentId: proof.assignmentId,
    holdingId: proof.holdingId,
  };
  const row = await linkScope(database, actor.companyId, proof.linkId);
  requireOwner(actor, row);
  if (row.binding_id !== proof.bindingId || row.set_id !== proof.setId)
    throw bindingChanged();
  await bindingCurrent(database, proof);
}
