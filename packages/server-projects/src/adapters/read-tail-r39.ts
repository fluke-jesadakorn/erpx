import {packageClockSql} from "@erpx/server-commercial/public/purchased-capability-sql";

/** Byte-identical scope bind-projection heads shared by the Projects assignment
 * scope, optional reference, workspace record and evaluation viewer reads. Only
 * the closed column key and trailing alias vary; bind order, parentheses and the
 * clock expression stay exactly as in the former inline literals. */
type ScopeBindKind = 'company' | 'id-company' | 'company-branch-department' | 'viewer-clock';
const scopeBindColumns: Record<ScopeBindKind, string> = {
  company: '?::text company_id',
  'id-company': '?::text id,?::text company_id',
  'company-branch-department': '?::text company_id,?::text branch_id,?::text department_id',
  'viewer-clock': `?::text user_id,?::text assignment_id,?::text role,?::text branch_id,?::text department_id,${packageClockSql} now_ms`,
};
export const scopeBindProjection = (kind: ScopeBindKind, alias = '') => {
  const columns = scopeBindColumns[kind];
  if (!columns) throw new Error(`Unknown Projects scope bind projection: ${kind}`);
  return `(SELECT ${columns})${alias ? ` ${alias}` : ''}`;
};

/** Byte-identical aliased staff_projects point-read tail shared by the Projects
 * member-scope guard, purchase source and attachment owner reads. Closed head and
 * shape keys own the projection columns, wrapper and whitespace gaps; the scope
 * was already caller-owned at every former site and its bind order is preserved.
 * No lock, join or native source text is emitted here. */
type PointReadKind = 'existence' | 'purchase-source' | 'attachment-owner';
type PointReadShape = 'member-guard' | 'purchase' | 'attachment';
type PointReadShapeSpec = {prefix: string;headGap: string;whereGap: string;beforeAnd: string;afterAnd: string;suffix: string};
const pointReadHeads: Record<PointReadKind, string> = {
  existence: '1',
  'purchase-source': 'p.id,p.company_id,p.branch_id,p.department_id,p.owner_user_id',
  'attachment-owner': 'p.company_id,p.branch_id,p.department_id,p.version,p.lifecycle_status,\n      CASE WHEN iam_same_person(p.owner_user_id,?,p.company_id) THEN 1 ELSE 0 END is_owner',
};
const pointReadShapes: Record<PointReadShape, PointReadShapeSpec> = {
  'member-guard': {prefix: 'EXISTS(', headGap: ' ', whereGap: ' ', beforeAnd: ' ', afterAnd: ' ', suffix: ')'},
  purchase: {prefix: '', headGap: ' ', whereGap: '\n ', beforeAnd: '\n ', afterAnd: ' ', suffix: ''},
  attachment: {prefix: '', headGap: '\n      ', whereGap: ' ', beforeAnd: ' ', afterAnd: '\n      ', suffix: ''},
};
export const projectPointReadSql = (kind: PointReadKind, shape: PointReadShape, scope: string) => {
  const head = pointReadHeads[kind], spec = pointReadShapes[shape];
  if (!head || !spec) throw new Error(`Unknown Projects point read: ${kind}/${shape}`);
  return `${spec.prefix}SELECT ${head}${spec.headGap}FROM staff_projects p${spec.whereGap}WHERE p.id=? AND p.company_id=?${spec.beforeAnd}AND${spec.afterAnd}${scope}${spec.suffix}`;
};
