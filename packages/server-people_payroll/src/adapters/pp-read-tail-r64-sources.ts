// Owner-local renderer for the byte-identical hr_time_entries id EXISTS guard
// head shared by two people_payroll call sites. The caller's predicate tail
// (minus the closing paren owned here) is passed through unchanged; binds
// and their order stay with the caller. Read-only.
export const ppRd64TimeEntryIdGuard = (tail: string) =>
  `EXISTS(SELECT 1 FROM hr_time_entries WHERE id=?${tail})`;
