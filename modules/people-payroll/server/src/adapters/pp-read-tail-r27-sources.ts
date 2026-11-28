import 'server-only';
/** Canonical parameterized SELECT-only owner read fragments shared across People Payroll
 * adapters (G4 read-tail r27). Each builder owns one read-only literal copied verbatim from
 * its former inline copies; consumers import it and keep no local copy. Every family here
 * is a pure read (no INSERT, UPDATE, DELETE, LOCK TABLE, FOR UPDATE/SHARE or advisory lock)
 * and is not covered by the r19-r26 canonical modules. Lock-bearing reads stay untouched. */

/** Calendar absence overlap witness. `listSql` is the r19 inList expression. */
export const ppRd27CalendarAbsenceSql = (listSql: string) =>
  `SELECT 1 FROM leave_requests l LEFT JOIN employees e ON e.id=l.employee_id AND e.company_id=l.company_id WHERE l.company_id=? AND l.state='approved' AND ${listSql} AND EXISTS(SELECT 1 FROM (
 SELECT s.leave_date::date absence_date FROM leave_day_segments s WHERE s.request_id=l.id AND s.company_id=l.company_id
 UNION ALL SELECT generate_series(l.start_date::date,coalesce(l.end_date::date,l.start_date::date+l.days::int-1),interval '1 day')::date WHERE NOT EXISTS(SELECT 1 FROM leave_day_segments s WHERE s.request_id=l.id)
 ) absence WHERE absence_date<=(to_timestamp((?::bigint-1)/1000.0) AT TIME ZONE ?)::date AND absence_date>=(to_timestamp(?::bigint/1000.0) AT TIME ZONE ?)::date) LIMIT 1`;

/** Leave-request list projection with caller-supplied where clauses. */
export const ppRd27LeaveRequestListSql = (clausesSql: string) =>
  `SELECT r.id,r.number,r.start_date,r.end_date,r.start_segment,r.end_segment,r.calculated_units,r.days,r.state,r.cancellation_state,r.version,r.created_at,r.updated_at,r.decision_note,t.name leave_type_name
      FROM leave_requests r LEFT JOIN leave_types t ON t.id=r.leave_type_id
      WHERE ${clausesSql} ORDER BY r.created_at DESC,r.id DESC LIMIT ?`;

/** id/company/version/status-literal guard; distinct from r21 (status before version) and r26 (bound status). */
export const ppRd27IdCompanyVersionStatusLiteralGuard = (table: string, status: string) =>
  `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND version=? AND status='${status}')`;

/** Narrow id/company point read; distinct from the r25 wildcard projection. */
export const ppRd27IdByIdCompanySql = (table: string) => `SELECT id FROM ${table} WHERE id=? AND company_id=?`;

/** id/company/engagement/approved/version guard. */
export const ppRd27IdCompanyEngagementApprovedVersionGuard = (table: string) =>
  `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND engagement_id=? AND status='approved' AND version=?)`;

/** id/version/status-in-list guard. */
export const ppRd27IdVersionStatusInGuard = (table: string, statuses: string) =>
  `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND version=? AND status IN (${statuses}))`;

/** Caller-scoped state/version/ownership point projection. */
export const ppRd27StateVersionScopeByIdSql = (table: string) =>
  `SELECT state,version,company_id,branch_id,department_id FROM ${table} WHERE id=?`;
