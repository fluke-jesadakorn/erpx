/** Owner-local byte-identical read SQL shared across Learning adapters. A single
 * text source keeps equivalent actor-scope, clock and course-image fragments
 * from drifting apart. */
import {packageClockSql} from "@erpx/server-commercial/public/purchased-capability-sql";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";

/** r16 — JSONB image aggregate head. The course, revocation and learner-assignment
 * images share the same to_jsonb(alias) ORDER BY alias.id head; only the alias
 * varies. Tails and binds stay with the callers, so every rendered image keeps
 * its exact pre-dedupe bytes. */
export const jsonbAggImageHead = (alias: string) => `SELECT jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.id)`;

/** r16 — scope row. The actor-scope, source-record and settlement-scope rows
 * share the same parenthesized column list; only the columns vary. Binds stay
 * with the callers, so every rendered row keeps its exact pre-dedupe bytes. */
export const scopeRow = (columns: string) => `(SELECT ${columns})`;

/** r17 — existence-probe head shared by the row-existence callers. The FROM
 * source and the WHERE body stay in the caller, so each rendered probe keeps
 * its exact pre-dedupe bytes and its own bind list. */
export const existsRowProbe = (source: string, where: string) => `SELECT 1 ${source} WHERE ${where}`;

/** r17 — keyed single-row read head. The projection, FROM/JOIN frame, key and
 * any tail stay with the caller; only the shared head bytes are owned here, so
 * every rendered read keeps its exact pre-dedupe text and binds. An empty key
 * means the caller supplies its own scope rows and no WHERE clause at all. */
export const keyedRowRead = (projection: string, frame: string, where = '', tail = '') =>
  `SELECT ${projection}${frame}${where}${tail}`;

export const actorScopeProbe = "(SELECT ?::text company_id,?::text branch_id,?::text department_id";
export const clockProbe = `MATERIALIZED (SELECT ${packageClockSql} now_ms)`;
export const coursesImage = `(${jsonbAggImageHead('c')} FROM learning_courses c WHERE c.company_id=?)`;

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
export const actorScopeExistsProbe = (columns: string, spacing = '') => `EXISTS(SELECT 1 FROM${spacing}${scopeRow(columns)}${spacing}learning_actor_scope WHERE ${assignmentReadSql('learning_actor_scope','records.read')})`;

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

/** r8b — cross-file learning_assignments existence probe. The head is
 * byte-identical at both callers; only the alias and conditions vary, so each
 * rendered query keeps its exact pre-dedupe bytes. */
export const learningAssignmentsExistsProbe = (alias: string, conditions: string) =>
  `EXISTS(SELECT 1 FROM learning_assignments ${alias} WHERE ${conditions})`;

/** r9 — the current assignment read-scope head is byte-identical at the metric
 * history and purchase authority callers: both select the same assignmentReadSql
 * projection and the same actorScopeProbe frame. Only the scope alias, the
 * permission and an optional grant clause differ, so one parameterized source
 * keeps both rendered queries equal. */
export const currentAssignmentScopeHead = (alias: string, permission: string, grant = '', separator = ' ') =>
  `SELECT ${assignmentReadSql(alias, permission)}${grant} allowed FROM${separator}${actorScopeProbe}) ${alias}`;

/** r10 — single-row learning_courses reads share the same FROM/WHERE tail and differ
 * only by projected columns. Only the projection and its separator vary, so every
 * rendered query keeps its exact pre-dedupe bytes.
 * r20 — that frame is exactly the r13 identitySourceRead head with key 'c.'; its one
 * query head now lives in that owner-local renderer. */
export const learningCourseProjection = (projection: string, separator = ' ') =>
  identitySourceRead(projection, 'learning_courses c', separator, 'c.');

/** r10 — assignment x course single-row reads share the same join head and differ
 * only by projected columns and WHERE tail. Only the projection and its separator
 * vary, so every rendered query keeps its exact pre-dedupe bytes. */
export const assignmentCourseJoinHead = (projection: string, separator = ' ') =>
  `SELECT ${projection}${separator}FROM learning_assignments a JOIN learning_courses c ON c.id=a.course_id AND c.company_id=a.company_id`;

/** r11 — single-row assignment x course reads that omit the company join key share
 * the same FROM/WHERE tail and differ only by projected columns and the whitespace
 * before FROM. Only the projection and its separator vary, so every rendered query
 * keeps its exact pre-dedupe bytes. */
export const assignmentCourseReadHead = (projection: string, separator = ' ') =>
  `SELECT ${projection}${separator}FROM learning_assignments a JOIN learning_courses c ON c.id=a.course_id WHERE a.id=? AND a.company_id=?`;

/** r12 — the package-obligation scope CTEs share the same parameterized SELECT
 * head. Only the scope alias and its source vary, so every rendered query keeps
 * its exact pre-dedupe bytes. */
export const packageScopeCte = (alias: string, source: string) =>
  `MATERIALIZED(SELECT ${alias}.* FROM ${source} ${alias},scope WHERE ${alias}.company_id=scope.company_id)`;

/** r13 — benefit consumption source head. The private witness and the
 * settlement source read the same receipt/certificate/handoff/assignment frame.
 * Only the projection and the two separator bytes vary, so each rendered query
 * keeps its exact pre-dedupe text. */
export const benefitConsumptionHead = (projection: string, separator = '\n ', joinSeparator = ' ') =>
  `SELECT ${projection}${separator}FROM learning_benefit_consumption_receipts r LEFT JOIN learning_certificates c ON c.id=r.certificate_id${joinSeparator}LEFT JOIN learning_eligibility_handoffs h ON h.id=r.handoff_id LEFT JOIN learning_assignments a ON a.id=c.assignment_id`;

/** r13 — latest-event LATERAL head. The assignment and course source reads
 * select their newest event with the same frame; only the projection, event
 * table, join key, parent alias and LATERAL separator vary. */
export const latestEventLateral = (projection: string, table: string, key: string, parent: string, separator = '') =>
  `LEFT JOIN LATERAL${separator}(SELECT ${projection} FROM ${table} e WHERE e.${key}=${parent}.id ORDER BY e.version DESC LIMIT 1) event ON TRUE`;

/** r13 — source identity read keyed by id and company. The command witness,
 * learner reference and sensitive assignment fact share the same head; only
 * the projection, source text, FROM separator and WHERE alias vary. */
export const identitySourceRead = (projection: string, source: string, separator = ' ', key = '') =>
  `SELECT ${projection}${separator}FROM ${source} WHERE ${key}id=? AND ${key}company_id=?`;

/** r13 — workspace image JSON frame. The purchase witness and the evaluation
 * image share the same jsonb_build_object(...)::text image frame; only the
 * object body varies. */
export const workspaceImageFrame = (body: string) => `SELECT jsonb_build_object(${body})::text image`;

/** r13 — clocked source CTE head. The three clocked read sites share the same
 * WITH frame; only the clock alias and the MATERIALIZED spacing vary. clockProbe
 * remains the single byte source for the clock body. */
export const clockedSourceCte = (alias: string, separator = ' ') =>
  `WITH ${alias} AS ${clockProbe.replace('MATERIALIZED (', `MATERIALIZED${separator}(`)}`;

/** r14 — package-obligation fact-row head. All five UNION ALL branches of the
 * settlement projection select the same four columns; only the union prefix,
 * the literal kind (with its first-branch alias) and the three projected
 * expressions vary. The 0::bigint revocation version stays literal at its
 * caller, so every rendered branch keeps its exact pre-dedupe bytes. */
export const obligationFactHead = (union: string, kind: string, fields: string) =>
  `${union}SELECT ${kind},${fields},`;

/** r15 — assignment-child image frame. The three nested child reads of the
 * attachment image share the same subquery framing; only the aggregate, source
 * and separator vary. Binds stay at the caller so each rendered frame keeps its
 * exact pre-dedupe bytes. */
export const imageChildFrame = (aggregate: string, source: string, separator = ' ') =>
  `(SELECT ${aggregate}${separator}${source})`;

/** r15 — certificate-consistency NOT EXISTS probe. The two settlement branches
 * share the same existence head; only the opening, FROM source, ordered tail
 * and closing parens vary, so each rendered check keeps its exact bytes. No
 * probe carries a bind; the enclosing query keeps its single company bind. */
export const certificateConsistencyProbe = (open: string, source: string, tail: string, close = ')') =>
  `${open}SELECT 1 ${source}${tail}${close}`;
