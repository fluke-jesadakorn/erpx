import 'server-only';
import {selectProjection} from './customer-authority-read-tail-r40-sources';

/**
 * One canonical closed binding/order read core shared by the Customer Authority
 * portal binding source (G4 read-tail r18) and portal purchase binding source
 * (G4 read-tail r19). Both prior helpers carried the same byte-identical
 * FROM/JOIN spine with `b`/`o` aliases and a closed `commerce_orders`
 * projection; only the caller-owned projection, whitespace separators and
 * trailing kind clause differ. The r40 universal projection head now opens the
 * core (G4 read-tail r46), so both renderers own that literal once and every
 * rendered statement keeps its exact pre-dedupe SQL bytes and prepared binds.
 * Pure read: no writes, no row/table locks, no advisory locks, no native
 * statement kinds and no eligibility changes. The r21-r45 builders, the
 * portal-access row-locked reader and the control-plane locks stay untouched.
 */
export const portalBindingOrderCore = (
  projection: string,
  fromSep: string,
  joinSep: string,
  orders: string,
  tail: string,
) => `${selectProjection(projection)}${fromSep}FROM customer_access_bindings b${joinSep}JOIN (${orders}) o ON o.id=b.order_id AND o.company_id=b.company_id AND o.party_id=b.customer_id${tail}`;
