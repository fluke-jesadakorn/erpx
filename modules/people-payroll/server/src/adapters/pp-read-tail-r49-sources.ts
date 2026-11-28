import 'server-only';
import { hospitalityOwnerProjectionSources as filesProjectionSources } from '@erpx/files-server/public/hospitality-owner-projection-sources';

/** Canonical parameterized SELECT-only owner read builders shared across People Payroll
 * adapters (G4 read-tail r49). Each builder owns one read-only literal copied from its
 * former inline copies; consumers import it and keep no local copy. Every family here is
 * a pure read (no INSERT/UPDATE/DELETE, LOCK TABLE, FOR UPDATE/SHARE or advisory lock)
 * and is not covered by the r1-r48 canonical modules. The attachmentsV2Readable()
 * branch stays at the consumer and each caller keeps its binds unchanged. Lock-bearing
 * reads, native kinds and cross-owner projections stay untouched. */

/** Expense-claim receipt V2 join form. The Files projection interpolations, revoked_at /
 * source_state / bound / set-target predicates are fixed owner fragments; each caller
 * keeps its single bind (owner id) and its attachmentsV2Readable() branch. */
export const ppRd49ExpenseClaimReceiptV2 = () =>
  `EXISTS(SELECT 1 FROM (${filesProjectionSources.attachments}) a JOIN (${filesProjectionSources.attachment_links}) l ON l.attachment_id=a.id JOIN (${filesProjectionSources.attachment_sets}) s ON s.id=l.set_id WHERE a.owner_type='expense_claim' AND a.owner_id=? AND a.revoked_at IS NULL AND a.source_state='available' AND l.state='bound' AND s.target_type='expense_claim' AND s.target_id=a.owner_id)`;

/** Expense-claim receipt V1 fallback. The Files projection and the revoked_at predicate
 * are fixed owner fragments; each caller keeps its single bind (owner id). */
export const ppRd49ExpenseClaimReceiptV1 = () =>
  `EXISTS(SELECT 1 FROM (${filesProjectionSources.attachments}) attachments WHERE owner_type='expense_claim' AND owner_id=? AND revoked_at IS NULL)`;

/** Attachment link point read by attachment id and set target. The Files projections,
 * company/set-target predicates and the three binds (attachment_id, company_id, target_id)
 * are fixed owner fragments; `targetType` selects the owner kind and `draft` keeps the
 * draft projection and the state='draft' predicate. */
export const ppRd49AttachmentLinkPoint = (targetType: string, draft = false) =>
  `SELECT l.id,${draft ? '' : 'l.state,'}l.set_id FROM (${filesProjectionSources.attachment_links}) l JOIN (${filesProjectionSources.attachment_sets}) s ON s.id=l.set_id WHERE l.attachment_id=? AND s.company_id=? AND s.target_type='${targetType}' AND s.target_id=?${draft ? " AND l.state='draft'" : ''}`;

/** Canonical-link draft guard. The Files projection and state='draft' predicate are fixed
 * owner fragments; each caller keeps its single bind (link id). */
export const ppRd49DraftLinkGuard = () =>
  `EXISTS(SELECT 1 FROM (${filesProjectionSources.attachment_links}) attachment_links WHERE id=? AND state='draft')`;
