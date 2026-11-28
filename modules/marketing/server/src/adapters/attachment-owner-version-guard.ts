import {marketingPointExistsSql} from './marketing-point-exists-sql';
/** Canonical owner-local fixed source-row guard frame for Marketing reads.
 * Every admitted source-row read passes an owner-owned table/subquery source
 * and a fixed WHERE tail; one builder emits the byte-identical guard so no
 * duplicate guard literal survives in the owner. Callers supply only
 * owner-owned fragments, never caller-authored SQL. r46: the guard wraps the
 * shared point-existence head instead of owning a second statement token. */
export const marketingSourceRowGuardSql = (source: string, tail: string): string =>
  `EXISTS(${marketingPointExistsSql(source, tail)})`;

/** Attachment owner-version guard, kept as the admitted wrapper over the
 * shared source-row frame. The service and operational attachment fact sources
 * call this with an owner table and an optional fixed extra predicate. */
export const attachmentOwnerVersionGuardSql = (table: string, extra = ""): string =>
  marketingSourceRowGuardSql(table, `id=? AND company_id=? AND version=?${extra ? ` AND ${extra}` : ""}`);
