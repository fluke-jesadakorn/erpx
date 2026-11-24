import 'server-only';

/** r48 additions: byte-identical read-tail SQL source. The helper renders
 * exactly the literal it replaces; callers keep their own binds, locks and
 * supplied statements. No r17-r47 helper, FOR SHARE/FOR UPDATE lock, LOCK TABLE
 * statement or spaced JOIN constant is changed. Only the owner-supplied
 * scope/kind predicate tail varies, never caller input. */

/** Authority-seat holder guard. Family: the "appointment-power" assertion in
 * powerGuard and the "flow-publisher" assertion in publishDepartmentFlow of
 * department-authority.ts. Each call renders exactly the replaced literal;
 * only the scope/kind predicate tail varies. */
export const authoritySeatHolderGuard = (people: string, engagements: string, tail: string) =>
  `EXISTS(SELECT 1 FROM authority_seats s JOIN (${people}) p ON p.id=s.person_id AND p.company_id=s.company_id JOIN (${engagements}) e ON e.person_id=p.id AND e.company_id=p.company_id AND e.status='active' WHERE s.company_id=? AND s.${tail})`;
