import { qualitySourceArmHead } from './quality-source-arm-head';
/** Owner-local return-anchor UNION arm shared by the Quality sales and
 * Ecommerce customer-return settlement reads. The four anchors differ only in
 * their projection, source relation and optional predicate; the statement head
 * and spacing are byte-identical to the former inline copy and are now shared
 * with the obligation-fact arms. Callers supply fixed owner-owned fragments,
 * no arm carries a bind, and no read here acquires a lock. */
export const qualityReturnAnchorSql = (
  columns: string,
  source: string,
  predicate = '',
) => qualitySourceArmHead(columns, source, predicate);
