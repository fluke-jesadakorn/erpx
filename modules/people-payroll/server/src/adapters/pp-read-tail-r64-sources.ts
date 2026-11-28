// Owner renderer for the hr_time_entries id EXISTS guard shared by two
// people_payroll call sites. The former inline head now renders from the shared
// round-2d template; the caller's predicate tail and binds stay unchanged. Read-only.
import { ppRdRound2dExistsById } from './pp-read-tail-round2d-sources';

export const ppRd64TimeEntryIdGuard = (tail: string) =>
  ppRdRound2dExistsById('hr_time_entries', tail);
