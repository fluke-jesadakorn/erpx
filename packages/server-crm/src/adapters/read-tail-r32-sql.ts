import { commercialOwnerProjectionSources as commerceProjectionSources } from "@erpx/server-commerce/public/commercial-owner-projection-sources";
import { crmProjectionFromSql } from './read-tail-r46-sql';

/** G4 / retire.crm.read-tail-r32 byte-identical read-head family.
 *
 * The quotes, sales and source-order readers share one commerce-order source
 * head: each selects a projection from the retained commerce-order projection
 * under its own alias and scope/order tail. The renderer below reproduces the
 * pre-dedupe inline SQL byte-for-byte; only the projection, alias and tail vary.
 * Locks, command-receipt reads, native command kinds and the r8-r31 families
 * are untouched. */
export const crmCommerceOrderReadSql = (projection: string, alias: string, tail: string) =>
  crmProjectionFromSql(projection, `(${commerceProjectionSources.commerce_orders}) ${alias} ${tail}`);
