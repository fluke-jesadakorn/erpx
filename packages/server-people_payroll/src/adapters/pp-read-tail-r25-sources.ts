import 'server-only';
import { ppRdRound4ByProjectionIdCompany } from './pp-read-tail-round4-sources';
/** Canonical parameterized SELECT-only owner read fragment shared across People Payroll
 * adapters (G4 read-tail r25). The builder owns one literal copied verbatim from its
 * former inline copies; consumers import it and keep no local copy. It is a pure read:
 * no INSERT, UPDATE, DELETE, LOCK TABLE, FOR UPDATE/SHARE or advisory lock. This family
 * is not covered by the r19-r24 canonical modules; prior constants untouched. */
/** Select-star by id/company. The identical head now renders from the round-4
 * owner builder; this name stays as the r25 consumer-facing alias. */
export const ppRd25ByIdCompany = (table: string) => ppRdRound4ByProjectionIdCompany('*', table);
