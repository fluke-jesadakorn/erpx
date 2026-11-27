import "server-only";
import { legalPointByKeySql } from './legal-read-tail-r36-sources';
/** Byte-identical disposed-document probe SQL shared by Legal owner adapters (round 20).
 * R69: the former inline head delegates to the verified r36 arbitrary-key point-read
 * renderer with the fixed '1' projection and 'document_id' key; the `=?` and ` IN (?,?)`
 * tails stay at each call site. Rendered text and every bind remain byte-identical. */
export const legalDisposedDocumentPrefix = legalPointByKeySql('1', 'legal_disposals', 'document_id').replace('=?', '');
