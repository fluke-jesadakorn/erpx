/** Owner-local byte-identical read SQL shared across Learning adapters. A single
 * text source keeps equivalent actor-scope, clock and course-image fragments
 * from drifting apart. */
import {packageClockSql} from "@erpx/server-commercial/public/purchased-capability-sql";

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
