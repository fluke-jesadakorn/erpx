import 'server-only';

/**
 * G4 / retire.customer_authority.read-tail-r45 byte-identical aggregate head,
 * widened at r47 to the shared aggregate prefix below.
 *
 * The r21 customer-document aggregate, the Portal attachment identity witness
 * and the r23 portal image subselects open the same closed JSONB aggregate
 * prefix and then append their own expression, ordering, source and closing
 * bytes. `jsonbAggHead` owns that prefix once (G4 read-tail r47) and both
 * renderers delegate to it, so every rendered statement keeps its exact
 * pre-dedupe SQL bytes, whitespace and prepared binds. Pure read: no writes,
 * no row/table locks, no advisory locks, no native statement kinds and no
 * authority-evaluation changes. The r21-r44 and r46 families, the row-locked
 * portal reader and the control-plane locks stay untouched.
 */
export const jsonbAggHead = (inner: string) => `(SELECT jsonb_agg(${inner}`;

export const jsonbAggregateHead = (body: string) => jsonbAggHead(`to_jsonb(${body}`);
