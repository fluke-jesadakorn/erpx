import 'server-only';
import { hospitalityOwnerProjectionSources as filesProjectionSources } from '@erpx/server-files/public/hospitality-owner-projection-sources';

/** Canonical parameterized SELECT-only owner read builders shared across People Payroll
 * adapters (G4 read-tail r50). Each builder owns one read-only literal copied verbatim
 * from its former inline copies; consumers import it and keep no local copy. Every
 * family here is a pure read (no INSERT/UPDATE/DELETE, LOCK TABLE, FOR UPDATE/SHARE
 * or advisory lock) and is not covered by the r1-r49 canonical modules. Lock-bearing
 * reads, native kinds, cross-owner projections and r1-r49 modules stay untouched. */

/** Executive COALESCE display-name label. The Organization projection and the id /
 * company_id predicates are fixed owner fragments; `alias`, `col`, `fallback` and
 * `from` carry the caller's exact bytes. There are no binds. The level/staff_levels
 * variant stays inline at its consumer. */
export const ppRd50CoalesceDisplayName = (alias: string, col: string, fallback: string, from: string) =>
  `COALESCE((SELECT display_name ${from} WHERE id=${alias}.${col} AND company_id=${alias}.company_id),${fallback})`;

/** Slip owner guard. The Files attachment projection and the id / revoked_at predicates
 * are fixed owner fragments; `ownerType` sets the literal owner kind and `ownerTypeFirst`
 * selects its exact predicate order. Each caller keeps its two binds (attachment id,
 * owner id). */
export const ppRd50SlipOwnerGuard = (ownerType: string, ownerTypeFirst = false) =>
  `EXISTS(SELECT 1 FROM (${filesProjectionSources.attachments}) attachments WHERE id=? AND ${ownerTypeFirst ? `owner_type='${ownerType}' AND owner_id=?` : `owner_id=? AND owner_type='${ownerType}'`} AND revoked_at IS NULL)`;
