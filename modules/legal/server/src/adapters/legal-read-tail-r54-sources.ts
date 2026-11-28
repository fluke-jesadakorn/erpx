import "server-only";
import { legalPointByKeySql } from './legal-read-tail-r36-sources';

/** R54 item-key head shared by Legal owner adapters (round 54).
 * Delegates to the verified r36 arbitrary-key head with the fixed item_id key and
 * an optional trusted second key. Callers append only fixed literal tails, keep
 * their own binds, and no request text reaches the head. No SQL text is inlined. */
export const legalItemKeyHeadSql = (
  columns: string,
  table: string,
  key2?: 'instance_id' | 'document_id' | 'created_by',
) => legalPointByKeySql(columns, table, 'item_id') + (key2 ? ` AND ${key2}=?` : '');

/** R54 ordered non-item-key head. Delegates to the widened r36 head; the caller
 * appends a fixed ORDER BY/LIMIT tail and keeps its own binds. No SQL text inlined. */
export const legalOrderedKeyHeadSql = (
  columns: string,
  table: string,
  key: 'destruction_id' | 'envelope_id' | 'item_id',
) => legalPointByKeySql(columns, table, key);
