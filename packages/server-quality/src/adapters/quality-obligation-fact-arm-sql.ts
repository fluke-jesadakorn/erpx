import { qualitySourceArmHead } from './quality-source-arm-head';
/** Owner-local obligation fact UNION arm shared by the Quality Supply and
 * Manufacturing package-obligation readers. The five arms differ only in their
 * projection, source relation and optional predicate; the statement head and
 * spacing are byte-identical to the former inline copy and are now shared with
 * the hold-source return anchors. Callers supply fixed owner-owned fragments, no
 * arm carries a bind, and no read here acquires a lock. */
export const qualityObligationFactArm = (
  columns: string,
  source: string,
  predicate = '',
) => qualitySourceArmHead(columns, source, predicate);
