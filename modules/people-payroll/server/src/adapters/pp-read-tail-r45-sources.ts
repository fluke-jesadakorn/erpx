import 'server-only';
/** Canonical read-only owner projection head builders shared across People Payroll
 * adapters (G4 read-tail r45). Each builder owns one literal pattern copied verbatim
 * from its former inline copies; consumers import it and keep no local copy. Every
 * family is a pure read (no writes, row locks or advisory locks) and is not covered
 * by the r1-r44 canonical modules. Lock-bearing reads, native kinds, cross-owner
 * projections and r1-r44 modules stay untouched. Bind order and call-site binds
 * are unchanged. */

/** Closed owner engagement projection head; the column list is the only fixed
 * owner fragment and the caller keeps no inline copy. */
export const ppRd45EngagementProjection = (cols: string) =>
  `SELECT ${cols} FROM hr_engagements`;

/** Payroll run point-read head; the column list is the fixed owner fragment and
 * the id/company predicates plus caller binds are unchanged. */
export const ppRd45RunByIdProjection = (cols: string) =>
  `SELECT ${cols} FROM hr_payroll_runs WHERE id=? AND company_id=?`;

/** Closed owner person projection head; the column list is the only fixed owner
 * fragment and the caller keeps no inline copy. */
export const ppRd45PeopleProjection = (cols: string) =>
  `SELECT ${cols} FROM hr_people`;

/** Compensation revision projection head; the column list is the fixed owner
 * fragment and the caller keeps no inline copy. */
export const ppRd45CompRevisionProjection = (cols: string) =>
  `SELECT ${cols} FROM hr_compensation_revisions`;

/** Command-receipt marker projection head; the selected alias and the marker key
 * are fixed owner fragments supplied by the caller. */
export const ppRd45ReceiptMarkerProjection = (cols: string, marker: string) =>
  `SELECT ${cols},authority_json::jsonb->>'companyId' company_id FROM command_receipts WHERE jsonb_exists(result_json::jsonb,'${marker}')`;
