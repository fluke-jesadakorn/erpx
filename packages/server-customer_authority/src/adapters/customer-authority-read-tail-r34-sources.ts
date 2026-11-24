import 'server-only';
import {finalRootOwnerProjectionSources as commerce} from '@erpx/server-commerce/public/final-root-owner-projection-sources';

/** One canonical binding-to-commerce-orders source core shared by the portal
 *  binding/purchase shells and the two remaining inline witness reads (G4
 *  read-tail r34). The projection, leading FROM tables and caller-owned trailing
 *  predicate are closed owner strings; every rendered statement keeps its exact
 *  pre-dedupe SQL bytes, prepared statement and binds. Pure read: no writes, no
 *  row/table locks and no advisory locks. The r21-r33 builders, portal-access
 *  row-locked reader and inline table-locked control plane stay untouched. */
export const bindingCommerceOrderSource = (projection: string, from: string, tail = '') =>
  `SELECT ${projection} FROM ${from} JOIN (${commerce.commerce_orders}) o ON o.id=b.order_id AND o.company_id=b.company_id${tail}`;

/** One canonical package-obligation UNION ALL fact-row scaffold shared by the
 *  four remaining inline fact rows (G4 read-tail r34). Caller-owned projection
 *  and source fragments keep each row's exact pre-dedupe SQL bytes and binds.
 *  Pure read; the table-locked control plane stays inline in the caller. */
export const unionFactRow = (projection: string, from: string) =>
  `SELECT ${projection} FROM ${from}`;
