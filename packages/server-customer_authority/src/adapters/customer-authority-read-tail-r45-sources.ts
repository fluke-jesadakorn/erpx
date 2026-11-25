import 'server-only';

/**
 * G4 / retire.customer_authority.read-tail-r45 byte-identical aggregate head.
 *
 * The r21 customer-document aggregate and the Portal attachment identity
 * witness open the same closed JSONB aggregate projection and then append their
 * own alias, ordering, source and closing parentheses. This renderer owns that
 * head once; callers pass only their own body so rendered SQL bytes, whitespace
 * and prepared binds stay exactly the same as the pre-dedupe literals. Pure
 * read: no writes, no row/table locks, no advisory locks, no native statement
 * kinds and no authority-evaluation changes. The r21-r44 families, the
 * row-locked portal reader and the control-plane locks stay untouched.
 */
export const jsonbAggregateHead = (body: string) => `(SELECT jsonb_agg(to_jsonb(${body}`;
