/** Owner-local completion-evidence projection shared by the native executive
 * evidence port and the transactional evidence port. Both callers render the
 * same column bytes; the remaining FROM/JOIN/WHERE is supplied by each caller.
 * This is a separate source from the r3 overdue/head helpers so those stay put. */
export const completionEvidenceColumns = 'SELECT e.id,e.assignment_id,e.version event_version,e.status,e.created_at,c.title course_title';
