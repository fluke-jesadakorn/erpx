// Owner-local renderer for the byte-identical hr_compensation_revisions
// engagement NO-EXISTS guard head shared by two people_payroll call sites.
// The caller's predicate tail (minus the closing paren owned here) is passed
// through unchanged; binds and their order stay with the caller. Read-only.
export const ppRd63CompRevisionEngagementGuard = (tail: string) =>
  `NOT EXISTS(SELECT 1 FROM hr_compensation_revisions WHERE engagement_id=?${tail})`;
