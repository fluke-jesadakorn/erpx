import "server-only";
import { legalShareDocumentReadHead } from './legal-read-tail-r41-sources';
import { legalPointByIdSql } from './legal-read-tail-r32-sources';

/** Family 1: the exact multi-line share/document image head shared by the two
 * portal source-image reads. It delegates to the verified r41 head, so every
 * table, alias and JOIN stays verbatim and no new bind or statement text is
 * introduced; the trailing whitespace is trimmed only to restore the original
 * line break before each caller's WHERE clause. */
export const legalShareDocumentImageHead = (projection: string) =>
  legalShareDocumentReadHead(projection).trimEnd();

/** Family 2: the exact clean-document guard head shared by the queued
 * completion and destruction-current reads. It delegates to the verified r32
 * point-read head and only prefixes the trusted document alias; the caller
 * supplies its own fixed trailing guard. No request value reaches the head,
 * no bind is added and the rendered SQL is byte-identical to every inline copy. */
export const legalCleanDocumentGuardHead = (alias: string, guard: string) => {
  const table = alias ? `legal_documents ${alias}` : 'legal_documents';
  const key = alias ? `${alias}.id` : 'id';
  const ref = alias ? `${alias}.` : '';
  return legalPointByIdSql('1', table).replace('WHERE id=?', `WHERE ${key}=?`) +
    ` AND ${ref}item_id=? AND ${ref}checksum=? AND ${guard}`;
};
