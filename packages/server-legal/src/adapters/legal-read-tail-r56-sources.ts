import "server-only";
import { legalPointByIdSql } from './legal-read-tail-r32-sources';
import { legalPointByKeySql } from './legal-read-tail-r36-sources';

/** R56 law point-guard and counsel-entry read heads. Every head delegates to a
 * verified base point-read source (r32 id-key / r36 arbitrary-key) so no SQL text
 * is inlined here and no bind is introduced. The only varied inputs are closed
 * table/key/predicate variants supplied by trusted callers; the fixed tails are
 * rendered verbatim and stay byte-identical to the former inline reads. */

/** Pending law-source-check guard: id=?, source_id=?, company_id=?, checksum=?. */
export const legalLawSourceCheckPendingSql = () =>
  legalPointByIdSql('1', 'legal_law_source_checks') +
  " AND source_id=? AND company_id=? AND state='pending_review' AND checksum=? AND candidate_revision_id IS NULL";

/** Active law-source guard: id=?, company_id=?, rule_id=? (bind order fixed). */
export const legalLawSourceCurrentSql = () =>
  legalPointByIdSql('1', 'legal_law_sources') + " AND company_id=? AND rule_id=? AND state='active'";

/** Pending law-source-check current guard: id=?, checksum=?. */
export const legalLawSourceCheckCurrentSql = () =>
  legalPointByIdSql('1', 'legal_law_source_checks') + " AND state='pending_review' AND checksum=? AND candidate_revision_id IS NULL";

/** Pending law-applicability guard: id=?, version=?, state='pending', same-person. */
export const legalLawApplicabilityCurrentSql = () =>
  legalPointByIdSql('1', 'legal_law_applicability') + " AND version=? AND state='pending' AND NOT iam_same_person(created_by,?,company_id)";

/** Law/document review-current guard: closed table variant plus fixed retiring tail. */
export const legalLawReviewCurrentSql = (
  table: 'legal_law_revisions' | 'legal_text_reviews',
  retiring: boolean,
) => legalPointByIdSql('1', table) + ` AND version=? AND state=? ${retiring ? "" : "AND NOT iam_same_person(created_by,?,company_id)"}`;

/** Unchanged law-source-check witness: source_id=?, id<>?, checksum=?, state IN-list. */
export const legalLawSourceCheckUnchangedSql = () =>
  legalPointByKeySql('1', 'legal_law_source_checks', 'source_id') + " AND id<>? AND checksum=? AND state IN ('unchanged','pending_review','reviewed') LIMIT 1";

/** Counsel conflict-cleared point read: item_id=? plus fixed entry-type/state tail. */
export const legalCounselConflictClearedSql = () =>
  legalPointByKeySql('1', 'legal_counsel_entries', 'item_id') + " AND data->>'entryType'='conflict' AND state IN ('approved','closed')";

/** Counsel approved-fee sum read: item_id=? plus fixed projection and tail. */
export const legalCounselApprovedFeesSql = () =>
  legalPointByKeySql("coalesce(sum(coalesce(nullif(data->>'amount',''),'0')::bigint),0) amount", 'legal_counsel_entries', 'item_id') + " AND id<>? AND state IN ('approved','billed','closed') AND data->>'entryType' IN ('fee','invoice')";
