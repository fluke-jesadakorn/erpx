import 'server-only';
import { hospitalityOwnerProjectionSources as filesProjectionSources } from '@erpx/server-files/public/hospitality-owner-projection-sources';
import { hospitalityOwnerProjectionSources as financeProjectionSources } from '@erpx/server-finance/public/hospitality-owner-projection-sources';
import { hospitalityOwnerProjectionSources as commerceProjectionSources } from '@erpx/server-commerce/public/hospitality-owner-projection-sources';

/** Canonical parameterized SELECT-only owner read builders shared across People Payroll
 * adapters (G4 read-tail r51). Each builder owns one read-only literal copied verbatim
 * from its former inline copies; consumers import it and keep no local copy. Every
 * family here is a pure read (no INSERT/UPDATE/DELETE, LOCK TABLE, FOR UPDATE/SHARE
 * or advisory lock) and is not covered by the r1-r50 canonical modules. Lock-bearing
 * reads, native kinds, cross-owner projections and r1-r50 modules stay untouched. */

/** Attachment list V2 join form. The Files projections and the fixed predicates are
 * owner fragments; `payment` carries the owner_id column, expense_payment_drafts join
 * and payment predicates, and `readOnly` selects the bound/draft-or-bound link state.
 * Each caller keeps its attachmentsV2Readable() branch and its original binds. */
export const ppRd51AttachmentListV2 = (payment: boolean, readOnly: boolean) =>
  `SELECT DISTINCT a.id,${payment ? 'a.owner_id,' : ''}a.original_name,a.media_type,a.bytes FROM (${filesProjectionSources.attachments}) a
             ${payment ? 'JOIN expense_payment_drafts d ON d.id=a.owner_id\n             ' : ''}JOIN (${filesProjectionSources.attachment_links}) l ON l.attachment_id=a.id JOIN (${filesProjectionSources.attachment_sets}) s ON s.id=l.set_id
             WHERE a.owner_type='${payment ? 'expense_payment' : 'expense_claim'}' AND ${payment ? 'd.expense_id=? AND d.company_id=?' : 'a.owner_id=?'} AND a.revoked_at IS NULL
               AND a.source_state='available' AND l.state${payment && !readOnly ? " IN ('draft','bound')" : "='bound'"}
               AND s.target_type='${payment ? 'expense_payment' : 'expense_claim'}' AND s.target_id=a.owner_id ORDER BY a.id`;

/** Attachment list V1 fallback. The Files projection and the fixed predicates are
 * owner fragments; `payment` carries the owner_id column, expense_payment_drafts join,
 * payment predicates and the a.created_at order alias. Each caller keeps its
 * attachmentsV2Readable() branch and its original binds. */
export const ppRd51AttachmentListV1 = (payment: boolean) =>
  `SELECT ${payment ? 'a.id,a.owner_id,a.original_name,a.media_type,a.bytes' : 'id,original_name,media_type,bytes'} FROM (${filesProjectionSources.attachments}) ${payment ? 'a JOIN expense_payment_drafts d ON d.id=a.owner_id' : 'attachments'} WHERE ${payment ? "a.owner_type='expense_payment' AND d.expense_id=? AND d.company_id=?" : "owner_type='expense_claim' AND owner_id=?"} AND ${payment ? 'a.' : ''}revoked_at IS NULL ORDER BY ${payment ? 'a.created_at' : 'created_at'}`;

/** Accounting-category chart guard. The Finance account and Commerce settings
 * projections and the id / company_id predicates are fixed owner fragments;
 * `categoryFirst` selects the exact category/status predicate order and `currencyTail`
 * selects the exact currency predicate bytes. Callers keep their 5/3/4 binds in order. */
export const ppRd51AccountingCategoryGuard = (categoryFirst: boolean, currencyTail: 'nullable-both' | 'account' | 'both') =>
  `EXISTS(SELECT 1 FROM (${financeProjectionSources.finance_accounts}) a JOIN (${commerceProjectionSources.commerce_settings}) s ON s.chart_id=a.chart_id AND s.company_id=a.company_id WHERE a.id=? AND a.company_id=? AND ${categoryFirst ? "a.category='expense' AND a.status='active'" : "a.status='active' AND a.category='expense'"} AND ${currencyTail === 'nullable-both' ? '(?::text IS NULL OR (a.currency=? AND s.currency=?))' : currencyTail === 'account' ? 'a.currency=?' : 'a.currency=? AND s.currency=?'})`;
