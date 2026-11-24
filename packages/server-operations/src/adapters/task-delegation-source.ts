import 'server-only';
/** Canonical single-row child JSON projection for the Operations task image.
 *  Table, alias, foreign key and outer alias are the only varying bytes; every
 *  rendering is byte-identical to the original inline literals. */
export const operationsChildJsonSql = (table:string,alias:string,fk:string,outer:string) => `(SELECT to_jsonb(${alias}) FROM ${table} ${alias} WHERE ${alias}.id=${outer}.${fk} AND ${alias}.company_id=${outer}.company_id)`;
/** Canonical task-scoped aggregated child JSON projection for the Operations
 *  task image. Table, alias and outer alias are the only varying bytes. */
export const operationsChildAggJsonSql = (table:string,alias:string,outer:string) => `(SELECT jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.id) FROM ${table} ${alias} WHERE ${alias}.task_id=${outer}.id AND ${alias}.company_id=${outer}.company_id)`;
/** Canonical source projections shared by the Operations task image SQL clauses. */
export const OPERATIONS_TASK_DELEGATION_SOURCE_SQL = operationsChildAggJsonSql('staff_task_delegations','delegation','t');
/** Canonical routine-template projection shared by the Operations task image SQL clauses. */
export const OPERATIONS_TASK_TEMPLATE_SOURCE_SQL = operationsChildJsonSql('routine_templates','template','template_id','t');
/** Canonical routine-schedule projection shared by the Operations task image SQL clauses. */
export const OPERATIONS_TASK_SCHEDULE_SOURCE_SQL = operationsChildJsonSql('routine_schedules','schedule','schedule_id','t');
/** Canonical approval-instance JSON image shared by the Operations task image SQL clauses.
 *  Key/alias/separator parameterized so each caller renders byte-identical SQL. */
export const operationsApprovalImageSql = (
  key: string,
  alias: string,
  sep: string,
  instances: string,
  steps: string,
  decisions: string,
) => `'${key}',(SELECT jsonb_agg(jsonb_build_object('instance',to_jsonb(${alias}),'steps',(SELECT jsonb_agg(to_jsonb(step) ORDER BY step.id) FROM (${steps})${sep}step WHERE step.instance_id=${alias}.id),'decisions',(SELECT jsonb_agg(to_jsonb(decision) ORDER BY decision.id) FROM (${decisions})${sep}decision WHERE decision.instance_id=${alias}.id)) ORDER BY ${alias}.id) FROM (${instances})${sep}${alias} WHERE ${alias}.record_id=t.id AND ${alias}.company_id=t.company_id)`;
/** Canonical active-delegation clock window shared by Operations task readers.
 *  Select/alias/source/clock/tail parameterized so each caller renders byte-identical SQL. */
export const operationsActiveDelegationSql = (
  select: string,
  alias: string,
  task: string,
  company: string,
  clock: string,
  tail = '',
  sep = ' ',
) => `SELECT ${select} FROM staff_task_delegations ${alias} WHERE ${alias}.task_id=${task} AND ${alias}.company_id=${company}${sep}AND ${alias}.status='active' AND ${alias}.starts_at<=${clock} AND ${alias}.expires_at>${clock}${tail}`;
