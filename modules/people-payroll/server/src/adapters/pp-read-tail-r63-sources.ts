// Owner-local renderer for the hr_compensation_revisions engagement NO-EXISTS
// guard shared by two people_payroll call sites. The former inline head now
// renders from the shared round-2b template; this pins the original table and
// predicate column and passes the caller's tail through unchanged. Read-only.
import { ppRdRound2bNotExistsByOnePred } from './pp-read-tail-round2b-sources';

export const ppRd63CompRevisionEngagementGuard = (tail: string) =>
  ppRdRound2bNotExistsByOnePred('hr_compensation_revisions', 'engagement_id', tail);
