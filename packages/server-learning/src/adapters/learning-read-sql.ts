/** Owner-local byte-identical read SQL shared across Learning adapters. A single
 * text source keeps equivalent actor-scope, clock and course-image fragments
 * from drifting apart. */
import {packageClockSql} from "@erpx/server-commercial/public/purchased-capability-sql";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";

export const actorScopeProbe = "(SELECT ?::text company_id,?::text branch_id,?::text department_id";
export const clockProbe = `MATERIALIZED (SELECT ${packageClockSql} now_ms)`;
export const coursesImage = "(SELECT jsonb_agg(to_jsonb(c) ORDER BY c.id) FROM learning_courses c WHERE c.company_id=?)";

/** The Learning source-union rows are emitted byte-identically by every ownership
 * probe; one text source keeps the native source and all callers equal. */
export const learningNativeSources=`SELECT id,company_id,NULL::text branch_id,NULL::text department_id,'learn.courses' capability FROM learning_courses
 UNION ALL SELECT e.id,c.company_id,NULL::text,NULL::text,'learn.courses' FROM learning_course_events e JOIN learning_courses c ON c.id=e.course_id
 UNION ALL SELECT id,company_id,branch_id,department_id,'learn.assignments' FROM learning_assignments
 UNION ALL SELECT e.id,a.company_id,a.branch_id,a.department_id,CASE WHEN e.action IN ('complete','waive') THEN 'learn.results' ELSE 'learn.assignments' END FROM learning_assignment_events e JOIN learning_assignments a ON a.id=e.assignment_id
 UNION ALL SELECT c.id,a.company_id,a.branch_id,a.department_id,'learn.certificates' FROM learning_certificates c JOIN learning_assignments a ON a.id=c.assignment_id AND a.company_id=c.company_id
 UNION ALL SELECT r.id,a.company_id,a.branch_id,a.department_id,'learn.certificates' FROM learning_certificate_revocations r JOIN learning_certificates c ON c.id=r.certificate_id AND c.company_id=r.company_id JOIN learning_assignments a ON a.id=c.assignment_id AND a.company_id=c.company_id
 UNION ALL SELECT h.id,a.company_id,a.branch_id,a.department_id,'learn.certificates' FROM learning_eligibility_handoffs h JOIN learning_assignments a ON a.id=h.assignment_id AND a.company_id=h.company_id`;

/** Native-source existence probe, parameterized only by the source alias; every
 * caller renders the same SQL bytes with its own alias. */
export const nativeSourceProbe = (alias: string) => `SELECT 1 FROM (${learningNativeSources}) ${alias}`;

/** The overdue evidence head is byte-identical in the executive evidence search
 * and the native executive projection; one source keeps both rendered queries equal. */
export const overdueEvidenceColumns = 'SELECT a.id,a.version,a.status,a.due_date,a.updated_at,c.title course_title,c.required_for';

/** Module-grant membership probe, parameterized only by the assignment alias. */
export const learningModuleGrantProbe = (alias: string) => `SELECT 1 FROM erpx_json_each(${alias}.modules_json) module WHERE module.value='learning'`;

/** r7 — current-assignment module-grant existence probe. The company binding and
 * aliasing stay in the caller's conditions; only the grant alias varies, so each
 * rendered query keeps the exact pre-dedupe bytes. */
export const moduleGrantAssignmentProbe = (alias: string, conditions: string) => `EXISTS(SELECT 1 FROM assignments ${alias} WHERE ${conditions} AND EXISTS(${learningModuleGrantProbe(alias)}))`;

/** r7 — actor-scope existence probe. The spacing parameter preserves the two
 * byte-identical renderings already emitted by the read tail. */
export const actorScopeExistsProbe = (columns: string, spacing = '') => `EXISTS(SELECT 1 FROM${spacing}(SELECT ${columns})${spacing}learning_actor_scope WHERE ${assignmentReadSql('learning_actor_scope','records.read')})`;

/** r7 — assignments image subquery, parameterized only by the projected JSON
 * expression so the guarded and unguarded images keep identical source bytes. */
export const assignmentsImage = (projection: string) => `(SELECT jsonb_agg(${projection} ORDER BY a.id) FROM learning_assignments a WHERE a.company_id=?)`;

/** r7 — literal SELECT * source text, parameterized only by the owner table. */
export const sourceLiteral = (table: string) => `SELECT * FROM ${table}`;

/** r8 — executive metric breakdown projection. The completion and overdue
 * branches emit the same projection head and GROUP BY tail byte-for-byte; only
 * the sourced FROM/WHERE differs, so each rendered query keeps its exact
 * pre-dedupe bytes. */
export const metricLabelProjection = (label: string, source: string) =>
  `SELECT ${label} label,count(*)::text value FROM ${source} GROUP BY 1 ORDER BY 1`;
