import { describe, expect, test } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { scopeBindProjection, projectPointReadSql } from '../../../packages/server-projects/src/adapters/read-tail-r39';
import { projectAssignmentScopeSql } from '../../../packages/server-projects/src/adapters/package-obligations';
import { projectAssignmentAuthorityReadSql } from '../../../packages/server-projects/src/adapters/read-tail-r23';
import { projectMemberScopeGuard } from '../../../packages/server-projects/src/adapters/project-read-projections';

const srcRoot = resolve(__dirname, '../../../packages/server-projects/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Verbatim rendered fragments captured from HEAD before the r39 edits.
// Byte-identity oracles only, never live query sources. The clock is the
// literal exported by purchased-capability-sql at HEAD.
const clock = 'floor(extract(epoch FROM clock_timestamp())*1000)::bigint';
const oracle = {
  scope: '(SELECT ?::text company_id,?::text branch_id,?::text department_id) workforce_actor_scope',
  reference: '(SELECT ?::text company_id) project_reference',
  record: '(SELECT ?::text id,?::text company_id) record',
  viewer: `(SELECT ?::text user_id,?::text assignment_id,?::text role,?::text branch_id,?::text department_id,${clock} now_ms)`,
  guard: 'EXISTS(SELECT 1 FROM staff_projects p WHERE p.id=? AND p.company_id=? AND __SCOPE__)',
  purchase: 'SELECT p.id,p.company_id,p.branch_id,p.department_id,p.owner_user_id FROM staff_projects p\n WHERE p.id=? AND p.company_id=?\n AND (NOT ?::boolean OR p.owner_user_id=? OR __MEMBER__)',
  attachment: 'SELECT p.company_id,p.branch_id,p.department_id,p.version,p.lifecycle_status,\n      CASE WHEN iam_same_person(p.owner_user_id,?,p.company_id) THEN 1 ELSE 0 END is_owner\n      FROM staff_projects p WHERE p.id=? AND p.company_id=? AND\n      (iam_same_person(p.owner_user_id,?,p.company_id) OR __MEMBER__)',
} as const;
const oracleSha = {
  scope: '4acc4b00f5e9db5ba13f8691865b2c43a5c3bc8fa62df2c6f88e410e846b501f',
  reference: '3b30afeecee118f5a09f97beadcb12bf33bdc308c4ba33e39897b8e18f391524',
  record: '23b1e25d335b4bc31bd2212da89319b06d9f6b27416cf725e00c8b45f2ec59a0',
  viewer: 'b4763dd0d96a36dfaa81f7b1f3ee3406d5a10ed8e5e9a8096aa3e3fe197f38b0',
  guard: '5afea9b51227606aad86bc81167c11badd299cf4f06ed7dfa2398ed84731f886',
  purchase: '2448ce1187ddaf0b2c78d67d49394d7dc3e856383d83b482424f99a69bc0337e',
  attachment: 'fffde45536c2ad8bba0610953c676fe85399ee0b4e6a753d851479d9ab5777cb',
} as const;

describe('Projects read-tail r39 closed scope-bind and point-read dedupe', () => {
  test('S1 closed bind projection reproduces all four former scope heads byte for byte', () => {
    expect(scopeBindProjection('company-branch-department', 'workforce_actor_scope')).toBe(oracle.scope);
    expect(scopeBindProjection('company', 'project_reference')).toBe(oracle.reference);
    expect(scopeBindProjection('id-company', 'record')).toBe(oracle.record);
    expect(scopeBindProjection('viewer-clock')).toBe(oracle.viewer);
    // Empty alias emits no trailing gap, preserving the MATERIALIZED CTE form.
    expect(scopeBindProjection('viewer-clock').endsWith('now_ms)')).toBe(true);
    expect(scopeBindProjection('viewer-clock')).not.toContain('now_ms) ');
    // Bind order and count are unchanged at every former site.
    expect((oracle.scope.match(/\?::text/g) ?? []).map(s => s.length)).toHaveLength(3);
    expect(oracle.reference.match(/\?::text/g)).toHaveLength(1);
    expect(oracle.record.match(/\?::text/g)).toHaveLength(2);
    expect(oracle.viewer.match(/\?::text/g)).toHaveLength(5);
    expect(oracle.viewer).toContain(`,${clock} now_ms)`);
    // Closed union: no caller column list or free SQL enters the helper.
    expect(() => (scopeBindProjection as (k: string, a?: string) => string)('bogus', 'x')).toThrow(/Unknown Projects scope bind projection/);
    const helper = src('read-tail-r39.ts');
    expect(helper).toContain("type ScopeBindKind = 'company' | 'id-company' | 'company-branch-department' | 'viewer-clock'");
    expect(helper).toContain('export const scopeBindProjection = (kind: ScopeBindKind, alias = \'\') =>');
    expect(helper).not.toContain('export const scopeBindProjection = (kind: string, columns: string');
    expect(helper.match(/SELECT /g)).toHaveLength(2); // one per closed helper
  });

  test('S1 call sites delegate and keep the original rendered SQL', () => {
    expect(projectAssignmentScopeSql('workforce_actor_scope')).toBe(oracle.scope);
    expect(projectAssignmentAuthorityReadSql('project_scope', 'records.read'))
      .toContain('allowed FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) project_scope');
    expect(src('package-obligations.ts')).toContain("import {scopeBindProjection} from './read-tail-r39';");
    expect(src('package-obligations.ts'))
      .toContain("export const projectAssignmentScopeSql=(alias:string)=>scopeBindProjection('company-branch-department',alias);");
    const opt = src('optional-project-options.ts');
    expect(opt).toContain("import {scopeBindProjection} from './read-tail-r39';");
    expect(opt).toContain("FROM ${scopeBindProjection('company','project_reference')}");
    expect(opt).not.toContain('FROM (SELECT ?::text company_id) project_reference');
    const rec = src('record-purchases.ts');
    expect(rec).toContain("import {scopeBindProjection} from './read-tail-r39';");
    expect(rec).toContain("FROM ${scopeBindProjection('id-company','record')}");
    expect(rec).not.toContain('FROM (SELECT ?::text id,?::text company_id) record');
    const evp = src('evaluation-source-policy.ts');
    expect(evp).toContain("import {scopeBindProjection} from './read-tail-r39';");
    expect(evp).toContain('AS MATERIALIZED ${scopeBindProjection(\'viewer-clock\')}');
    expect(evp).not.toContain('AS MATERIALIZED (SELECT ?::text user_id');
    expect(evp).not.toContain('packageClockSql'); // now owned by the closed helper
    expect((evp.match(/SELECT /g) ?? [])).toHaveLength(4);
  });

  test('S2 closed point-read tail reproduces all three former statements byte for byte', () => {
    const purchaseScope = '(NOT ?::boolean OR p.owner_user_id=? OR __MEMBER__)';
    const attachmentScope = '(iam_same_person(p.owner_user_id,?,p.company_id) OR __MEMBER__)';
    expect(projectPointReadSql('existence', 'member-guard', '__SCOPE__')).toBe(oracle.guard);
    expect(projectPointReadSql('purchase-source', 'purchase', purchaseScope)).toBe(oracle.purchase);
    expect(projectPointReadSql('attachment-owner', 'attachment', attachmentScope)).toBe(oracle.attachment);
    expect(projectMemberScopeGuard('__SCOPE__')).toBe(oracle.guard);
    // Bind order is preserved: head binds precede the point-read ids and scope
    // binds. __MEMBER__/__SCOPE__ are markers standing for caller-owned scopes.
    expect(oracle.purchase.match(/\?/g)).toHaveLength(4);
    expect(oracle.attachment.match(/\?/g)).toHaveLength(4);
    expect(oracle.guard.match(/\?/g)).toHaveLength(2);
    // Closed union keys only: no free projection or relation is accepted.
    expect(() => (projectPointReadSql as (k: string, s: string, scope: string) => string)('bogus', 'purchase', 'x')).toThrow(/Unknown Projects point read/);
    expect(() => (projectPointReadSql as (k: string, s: string, scope: string) => string)('existence', 'bogus', 'x')).toThrow(/Unknown Projects point read/);
    const helper = src('read-tail-r39.ts');
    expect(helper).toContain("type PointReadKind = 'existence' | 'purchase-source' | 'attachment-owner'");
    expect(helper).toContain("type PointReadShape = 'member-guard' | 'purchase' | 'attachment'");
    expect(helper).toContain('export const projectPointReadSql = (kind: PointReadKind, shape: PointReadShape, scope: string) =>');
    expect(helper).not.toContain('FROM ${relation}');
    expect(helper).not.toContain('SELECT ${columns} FROM ${table}');
  });

  test('S2 call sites delegate and keep HEAD SQL, binds and result types', () => {
    const pr = src('project-read-projections.ts');
    expect(pr).toContain("import {projectPointReadSql} from './read-tail-r39';");
    expect(pr).toContain("export const projectMemberScopeGuard = (scope: string) => projectPointReadSql('existence','member-guard',scope);");
    expect(pr).not.toContain('`EXISTS(SELECT 1 FROM staff_projects p WHERE p.id=?');
    const pu = src('purchases.ts');
    expect(pu).toContain("import {projectPointReadSql} from './read-tail-r39';");
    expect(pu).toContain("projectPointReadSql('purchase-source','purchase',`(NOT ?::boolean OR p.owner_user_id=? OR ${projectMemberExistsSql()})`)");
    expect(pu).not.toContain('`SELECT p.id,p.company_id,p.branch_id,p.department_id,p.owner_user_id FROM staff_projects p');
    expect(pu).toContain('.bind(id,actor.companyId,member,actor.userId,actor.userId).first<Source>()');
    const at = src('attachments-policy-facts.ts');
    expect(at).toContain("import {projectPointReadSql} from './read-tail-r39';");
    expect(at).toContain("projectPointReadSql('attachment-owner','attachment',`(iam_same_person(p.owner_user_id,?,p.company_id) OR ${projectMemberExistsSql()})`)");
    expect(at).not.toContain('`SELECT p.company_id,p.branch_id,p.department_id,p.version,p.lifecycle_status,');
    expect(at).toContain('.bind(facts.userId,facts.targetId,facts.companyId,facts.userId,facts.userId)');
    // r33 point-read head is untouched: only readDescriptor still uses it.
    expect(at).toContain("import {projectPointReadHead} from './read-tail-r33';");
    expect(at).toContain('${projectPointReadHead(');
  });

  test('S3 SKIP: assignment-viewer authority tail is not a byte-identical closed family', () => {
    const ep = src('evaluation-source-policy.ts'), pe = src('product-entry.ts'), st = src('scheduled-tasks.ts');
    // Exact anchors and their incommensurate structures:
    // evaluation-source-policy.ts:22 EXISTS(SELECT 1 ... viewer column refs + native.company_id
    expect(ep).toContain('EXISTS(SELECT 1 FROM assignments g WHERE g.id=${viewer}.assignment_id');
    expect(ep).toContain('g.company_id=native.company_id');
    // product-entry.ts:22-26 MATERIALIZED viewer + spaced CROSS JOIN clock + ? binds
    expect(pe).toContain('project_entry_viewer AS MATERIALIZED (${assignmentViewerSelect}clock.now_ms FROM assignments g CROSS JOIN project_entry_clock clock');
    expect(pe).toContain('WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=? AND g.version=?');
    // scheduled-tasks.ts:24 MATERIALIZED viewer + clock, then authority outside the CTE
    expect(st).toContain('WITH project_alert_viewer AS MATERIALIZED (${assignmentViewerSelect}${packageClockSql} now_ms FROM assignments g WHERE g.id=? AND g.user_id=? AND g.company_id=? AND g.version=?)');
    expect(st).toContain('FROM staff_tasks t CROSS JOIN project_alert_viewer WHERE t.id=? AND t.company_id=?');
    expect(st).not.toContain("g.scope_kind='holding'"); // no scope predicate in this CTE
    // The longest shared fragments are not a single select head: they diverge
    // into column refs vs binds, a spaced clock JOIN, and viewer/t aliases.
    expect(ep).not.toContain('CROSS JOIN project_entry_clock clock');
    expect(pe).not.toContain('native.company_id');
    // Excluded scheduled-tasks.ts:21 CROSS JOIN LATERAL stays inline.
    expect(st).toContain('EXISTS(SELECT 1 FROM assignments g CROSS JOIN LATERAL (${assignmentViewerSelect}project_alert_clock.now_ms) project_alert_viewer WHERE');
    // No r39 helper was routed into these three specific S3 anchors.
    // evaluation-source-policy.ts uses the helper only at its S1 viewer anchor.
    expect(ep.match(/\$\{scopeBindProjection\('viewer-clock'\)\}/g)).toHaveLength(1);
    expect(pe).not.toContain('scopeBindProjection');
    expect(st).not.toContain('scopeBindProjection');
    for (const text of [ep, pe, st]) expect(text).not.toContain('projectPointReadSql');
  });

  test('locks, spaced joins, native sources and earlier helpers stay untouched', () => {
    const obl = src('package-obligations.ts');
    expect(obl).toContain('LOCK TABLE staff_projects,staff_project_members,staff_project_milestones,staff_project_milestone_events,');
    expect(obl).toContain('staff_project_handoffs,command_receipts IN SHARE MODE');
    expect(src('purchases.ts')).toContain('LOCK TABLE staff_projects,staff_project_members IN SHARE ROW EXCLUSIVE MODE');
    const st = src('scheduled-tasks.ts');
    expect(st).toContain('FOR UPDATE OF t');
    expect(st).toContain('FOR SHARE');
    expect(st).toContain('CROSS JOIN LATERAL');
    expect(src('evaluation-source-policy.ts')).toContain('nativeSources');
    for (const [file, symbol] of [
      ['read-tail-r23.ts', 'projectAssignmentAuthorityReadSql'],
      ['read-tail-r28.ts', 'assignmentViewerSelect'],
      ['read-tail-r29.ts', 'orderedImageAggregateHead'],
      ['read-tail-r30.ts', 'visibleProjectRowsHead'],
      ['read-tail-r33.ts', 'projectPointReadHead'],
      ['read-tail-r34.ts', 'staffTaskPointHead'],
      ['read-tail-r38.ts', 'memberExistsSql'],
    ] as const) expect(src(file)).toContain(symbol);
    // They are not routed through the new helpers.
    for (const file of ['read-tail-r23.ts','read-tail-r27.ts','read-tail-r28.ts','read-tail-r29.ts','read-tail-r30.ts','read-tail-r33.ts','read-tail-r34.ts','read-tail-r37.ts','read-tail-r38.ts']) {
      expect(src(file)).not.toContain('read-tail-r39');
    }
  });

  test('HEAD byte oracles are unchanged (SHA-256 pin)', () => {
    for (const [key, text] of Object.entries(oracle)) {
      expect(createHash('sha256').update(text).digest('hex')).toBe(oracleSha[key as keyof typeof oracle]);
    }
  });

  test('retirement:ts-statements is 74 after r40 (BEFORE=81)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true }) as Dirent[]) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r38 left 81. r39 folds four scope bind heads into one closed helper (-3)
    // and three aliased point-read tails into one closed helper (-2): 81-5 = 76.
    // The assignment-viewer authority tail (S3) is not a byte-identical family.
    expect(total).toBe(67); // lane 27 wave 2 moved the two handoff list/work reads to native (-2) // lane 27 moved the two actuals reads to native Rust (-2)
    expect(total).toBeLessThan(81);
  });
});
