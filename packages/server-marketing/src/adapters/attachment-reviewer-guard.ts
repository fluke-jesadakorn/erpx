import 'server-only';
import {assignmentReadSql} from '@erpx/server-identity_access/public/record-source-authority';
import {actorScopeAssignmentSql} from './marketing-actor-scope-guard';
/** Canonical owner-local fixed guard for Marketing attachment revocation review.
 * The immediate approver check and the command assertion share this one
 * admitted predicate; a caller supplies only an optional projection alias,
 * never authored SQL or a relation name. */
export const attachmentReviewerGuardSql = (alias = ''): string =>
  actorScopeAssignmentSql(`(${assignmentReadSql('marketing_file_review','records.review')} OR ${assignmentReadSql('marketing_file_review','commerce.approve')})`,'marketing_file_review',alias ? ` ${alias}` : '');
