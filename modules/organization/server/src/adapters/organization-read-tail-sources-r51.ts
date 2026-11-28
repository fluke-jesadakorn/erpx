import 'server-only';

/** r51 additions: byte-identical read-tail SQL source. Each helper renders
 * exactly the literal it replaces; callers keep their own binds, locks and
 * supplied statements. No r17-r50 helper, FOR SHARE/FOR UPDATE lock, LOCK TABLE
 * statement or spaced JOIN constant is changed. Projections are closed to the
 * three owner-owned families; the owner-supplied tails stay at the call sites
 * and never carry caller input. */

/** Authority-seat company-scoped read. Family: the relocation head snapshot,
 * the legacy HR transition deadline and the approvals appointed-step probe.
 * Only the closed projection and the verbatim predicate tail vary. */
export type AuthoritySeatProjection =
  | 'version,position_id,person_id,effective_at'
  | 'min(effective_at) AS deadline'
  | '1';
export const authoritySeatCompanyRead = (projection: AuthoritySeatProjection, tail: string) =>
  `SELECT ${projection} FROM authority_seats WHERE company_id=? AND ${tail}`;

/** Active department node by company and id. Family: the organization port
 * department-active probe and the GPO department target name lookup. The
 * optional branch predicate stays in the caller's tail. */
export type DepartmentNodeProjection = '1' | 'display_name name';
export const activeDepartmentNode = (projection: DepartmentNodeProjection) =>
  `SELECT ${projection} FROM organization_nodes WHERE company_id=? AND id=? AND kind='department' AND status='active'`;

/** Member-profile version guard. Family: the media upload and media removal
 * assertions. The optional key predicate stays in the caller's tail. */
export const memberProfileVersionGuard = (tail: string) =>
  `EXISTS(SELECT 1 FROM member_profiles WHERE user_id=? AND version=?${tail})`;
