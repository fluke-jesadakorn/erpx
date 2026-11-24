import 'server-only';

/** One canonical Support-conclusion identity projection shared by the
 *  acknowledgement row and the closed-without-acknowledgement row of the
 *  Customer Authority package-obligation conclusion proof (G4 read-tail r31).
 *  Only the closed row alias and outcome expression are supplied; each caller
 *  keeps its exact FROM/WHERE text, prepared statement and binds, so the
 *  rendered SQL stays byte-identical to the pre-dedupe literal. This family is
 *  a pure read: no writes, no row or table locks and no advisory locks. The
 *  r21-r30 builders, the control-plane lock statement, native-owned fact kinds
 *  and the manager-eligibility helpers stay untouched. */
export const supportConclusionIdentity = (alias: string, outcome: string) =>
  `SELECT ${alias}.request_id,${alias}.company_id,${alias}.id,${outcome}`;
