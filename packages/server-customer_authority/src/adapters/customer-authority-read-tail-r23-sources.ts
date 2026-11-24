/** One canonical JSONB image-projection subselect shared by the customer
 *  document read's identity and read-audit tails (G4 read-tail r23). The object
 *  list, ORDER BY expression and source tail are closed, owner-local strings;
 *  every rendered fragment stays byte-identical to its pre-dedupe literal.
 *  Callers keep their own prepared statements, binds and source leases. This
 *  family is a pure read: no writes, no row/table locks and no advisory locks.
 *  The r21 `customer-authority-read-sql` and r22 `package-obligation-sql`
 *  builders are not part of this family. */
export const jsonbImageProjection = (object: string, order: string, tail: string) =>
  `(SELECT jsonb_agg(jsonb_build_object(${object}) ORDER BY ${order}) ${tail})`;
