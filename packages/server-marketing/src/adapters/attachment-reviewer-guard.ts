import 'server-only';
import {assignmentReadSql} from '@erpx/server-identity_access/public/record-source-authority';
/** Canonical owner-local fixed guard for Marketing attachment revocation review.
 * The immediate approver check and the command assertion share this one
 * admitted predicate; a caller supplies only an optional projection alias,
 * never authored SQL or a relation name. */
export const attachmentReviewerGuardSql = (alias = ''): string =>
  `SELECT (${assignmentReadSql('marketing_file_review','records.review')} OR ${assignmentReadSql('marketing_file_review','commerce.approve')})${alias ? ` ${alias}` : ''} FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) marketing_file_review`;
