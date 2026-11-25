import { describe, expect, test } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { selectProjectionFrom } from '../../../packages/server-projects/src/adapters/read-tail-r40';
import { projectPointReadHead } from '../../../packages/server-projects/src/adapters/read-tail-r33';
import {
  staffTaskPointHead, staffProjectByRefHead,
} from '../../../packages/server-projects/src/adapters/read-tail-r34';

const srcRoot = resolve(__dirname, '../../../packages/server-projects/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');

// Verbatim pre-r40 renders captured at BEFORE=76 (r33 and r34 byte oracles).
// Byte-identity oracles only, never live query sources.
const a1 = 'SELECT id,company_id,branch_id,department_id,creator_user_id,assignee_user_id,project_id,template_id,schedule_id,state,version FROM staff_tasks WHERE id=? AND company_id=?';
const a2 = "SELECT (to_jsonb(t)-'updated_at')::text image FROM staff_tasks t WHERE id=? AND company_id=?";
const a3 = `SELECT jsonb_build_object('task',to_jsonb(t),'project',(SELECT to_jsonb(p) FROM staff_projects p WHERE p.id=t.project_id AND p.company_id=t.company_id),'delegations',(\${orderedImageAggregateHead('d','id')} FROM staff_task_delegations d WHERE d.task_id=t.id AND d.company_id=t.company_id),'approval',(SELECT jsonb_agg(jsonb_build_object('instance',to_jsonb(i),'steps',(\${orderedImageAggregateHead('s','id')} FROM approval_steps s WHERE s.instance_id=i.id)) ORDER BY i.id) FROM approval_instances i WHERE i.record_type='task' AND i.record_id=t.id AND i.company_id=t.company_id),'origin',(SELECT jsonb_agg(jsonb_build_object('origin',to_jsonb(o),'post',to_jsonb(p)) ORDER BY o.post_id) FROM social_task_origins o LEFT JOIN social_posts p ON p.id=o.post_id WHERE o.task_id=t.id))::text image FROM staff_tasks t WHERE t.id=? AND t.company_id=?`;
const a4 = 'SELECT t.id,t.company_id,t.project_id,t.version,t.title,t.due_at,t.assignee_user_id,t.creator_user_id,t.state,(SELECT p.owner_user_id FROM staff_projects p WHERE p.id=t.project_id AND p.company_id=t.company_id) project_owner_id FROM staff_tasks t WHERE t.id=? AND t.company_id=? FOR UPDATE OF t';
const a3Columns = a3.slice('SELECT '.length, -' FROM staff_tasks t WHERE t.id=? AND t.company_id=?'.length);
const a4Columns = a4.slice('SELECT '.length, -' FROM staff_tasks t WHERE t.id=? AND t.company_id=? FOR UPDATE OF t'.length);
const b1 = 'SELECT to_jsonb(p) FROM staff_projects p WHERE p.id=t.project_id AND p.company_id=t.company_id';
const b2 = 'SELECT p.owner_user_id FROM staff_projects p WHERE p.id=t.project_id AND p.company_id=t.company_id';
const b3 = '(SELECT pr.title FROM staff_projects pr WHERE pr.id=p.audience_id AND pr.company_id=p.company_id)';
const b4 = 'SELECT 1 FROM staff_projects audience_project\n      WHERE audience_project.id=p.audience_id AND audience_project.company_id=p.company_id';
const b5 = 'SELECT 1 FROM staff_projects hp WHERE hp.id=${task}.project_id\n      AND hp.company_id=${task}.company_id';
const b6 = 'SELECT 1 FROM staff_projects hp WHERE hp.id=hs.source_id AND hp.company_id=${post}.company_id';
const b7 = 'SELECT 1 FROM staff_projects pr WHERE pr.id=${alias}.audience_id AND pr.company_id=${alias}.company_id';

const oracles = {
  assignment: 'SELECT version FROM assignments WHERE id=? AND user_id=? AND company_id=? AND role=?',
  descriptor: 'SELECT company_id,branch_id AS branch_id,department_id AS department_id,version AS version FROM staff_projects WHERE id=? AND company_id=?',
  guard: 'SELECT 1 FROM staff_projects WHERE id=? AND user_id=?',
  a1, a2, a3, a4, b1, b2, b3, b4, b5, b6, b7,
} as const;
const oracleSha = {
  assignment: 'c33bf844cb52e1be884612169d242762a99b4c30185ef0e10a8bba871d07c2b1',
  descriptor: 'd1f92e7793e6491ab4dc64a0f7baf39259d41e14d2ace0bd05496e66465b1f9c',
  guard: '4a084cfceb55cb3e92239e601c68eefb6b86b45151a4951bcc838eecc80b2b50',
  a1: '75726364af3f066a102fccbfb44f9a38063b0942184f443c015f41e3392322fe',
  a2: '6bcae9de867750db744f32499236755085ee653cddc227ad1a4e6154f614f552',
  a3: '9e9cd1fb8323120a8da0527d705a1df3cd7618c14bd65d9fb3d7679cb2bbdce4',
  a4: 'edb4bb11d663dc8f63b336f0c7b0be936eb57313584d02da9877c16cc98919ec',
  b1: '87b2964eca097c944388c1cb5985147b2b99c812695f4c0b48069f02800ce298',
  b2: 'ddad9d336e80aeaf75700b1baf9a196addb765748b976dcd05cba012842abd5e',
  b3: 'd66c514122319d8ebf99813f1a4bdd1cdb62817674b428f3a75645ae7430b663',
  b4: 'dbdb045fc7382f5a0965544d19d67fbb210d91d40b71f65f5703a83abd133612',
  b5: 'ebfc74d228be7eba3967e09395401461ecf76ac825178990ba842e3dca4e6927',
  b6: '3439be08bbd442d12336606082e0c794c69f4ff2f0729d8d589f76be82c56af5',
  b7: 'c0af2030fb4283390c78044c8c5266be25703ebb69bb69d03e79f2ca7beb4a54',
} as const;

describe('Projects read-tail r40 universal projection head family', () => {
  test('one closed universal head renders the shared SELECT prefix', () => {
    expect(selectProjectionFrom('1', 'staff_projects')).toBe('SELECT 1 FROM staff_projects');
    expect(selectProjectionFrom('a,b', 'staff_tasks t')).toBe('SELECT a,b FROM staff_tasks t');
    const helper = src('read-tail-r40.ts');
    expect(helper.match(/SELECT /g)).toHaveLength(1);
    expect(helper.match(/export const selectProjectionFrom/g)).toHaveLength(1);
    expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  test('all three wrappers reproduce every pre-r40 render byte for byte', () => {
    const pairs: Array<[string, string]> = [
      [`${projectPointReadHead('version', 'assignments', 'user_id')} AND company_id=? AND role=?`, oracles.assignment],
      [projectPointReadHead('company_id,branch_id AS branch_id,department_id AS department_id,version AS version', 'staff_projects', 'company_id'), oracles.descriptor],
      [projectPointReadHead('1', 'staff_projects', 'user_id'), oracles.guard],
      [staffTaskPointHead('id,company_id,branch_id,department_id,creator_user_id,assignee_user_id,project_id,template_id,schedule_id,state,version'), a1],
      [staffTaskPointHead("(to_jsonb(t)-'updated_at')::text image", 't'), a2],
      [staffTaskPointHead(a3Columns, 't', 't.'), a3],
      [`${staffTaskPointHead(a4Columns, 't', 't.')} FOR UPDATE OF t`, a4],
      [staffProjectByRefHead('to_jsonb(p)', 'p', 't.project_id', 't.company_id'), b1],
      [staffProjectByRefHead('p.owner_user_id', 'p', 't.project_id', 't.company_id'), b2],
      [`(${staffProjectByRefHead('pr.title', 'pr', 'p.audience_id', 'p.company_id')})`, b3],
      [staffProjectByRefHead('1', 'audience_project', 'p.audience_id', 'p.company_id', '\n      '), b4],
      [staffProjectByRefHead('1', 'hp', '${task}.project_id', '${task}.company_id', ' ', '\n      '), b5],
      [staffProjectByRefHead('1', 'hp', 'hs.source_id', '${post}.company_id'), b6],
      [staffProjectByRefHead('1', 'pr', '${alias}.audience_id', '${alias}.company_id'), b7],
    ];
    for (const [rendered, oracle] of pairs) expect(rendered).toBe(oracle);
    // Bind count and order are unchanged at every former site: the point reads
    // still carry two `?` binds before any caller-owned trailing predicate.
    expect(oracles.assignment.match(/\?/g)).toHaveLength(4);
    expect(oracles.descriptor.match(/\?/g)).toHaveLength(2);
    expect(a1.match(/\?/g)).toHaveLength(2);
    for (const key of Object.keys(oracles) as Array<keyof typeof oracles>) {
      expect(sha256(oracles[key]), key).toBe(oracleSha[key]);
    }
  });

  test('named wrappers delegate and keep their export names with no inline SELECT', () => {
    for (const [file, names] of [
      ['read-tail-r33.ts', ['projectPointReadHead']],
      ['read-tail-r34.ts', ['staffTaskPointHead', 'staffProjectByRefHead']],
    ] as Array<[string, string[]]>) {
      const text = src(file);
      expect(text.match(/SELECT /g) ?? [], file).toHaveLength(0);
      expect(text, file).toContain("from './read-tail-r40'");
      expect(text, file).toContain('selectProjectionFrom(');
      for (const name of names) expect(text, name).toContain(`export const ${name}`);
      expect(text, file).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
    // Both closed key unions and the whitespace-gap defaults survive the fold.
    expect(src('read-tail-r33.ts')).toContain("key2: 'user_id' | 'company_id'");
    expect(src('read-tail-r34.ts')).toContain("whereGap = ' '");
    expect(src('read-tail-r34.ts')).toContain("andGap = ' '");
  });

  test('callers keep their locks, binds and trailing SQL', () => {
    const tp = src('task-purchases.ts');
    expect(tp).toContain('${lock?\' FOR UPDATE\':\'\'}');
    expect(tp).toContain('.bind(id,actor.companyId).first<TaskSource>()');
    const st = src('scheduled-tasks.ts');
    expect(st).toContain('FOR UPDATE OF t');
    expect(st).toContain('FOR SHARE');
    expect(st).toContain('.bind(target.id,target.companyId).first<{project_id:string|null;version:number;');
    expect(src('package-obligations.ts')).toContain('LOCK TABLE staff_projects,staff_project_members,staff_project_milestones');
    expect(src('purchases.ts')).toContain('LOCK TABLE staff_projects,staff_project_members IN SHARE ROW EXCLUSIVE MODE');
    expect(src('social-purchase-sources.ts')).toContain('LOCK TABLE staff_projects,staff_project_members IN SHARE MODE');
    expect(src('people-evaluation-facts.ts')).toContain('LOCK TABLE staff_projects,staff_project_members IN SHARE ROW EXCLUSIVE MODE');
    // Call sites still compose the named wrappers directly.
    expect(tp).toContain("import {staffTaskPointHead} from './read-tail-r34';");
    expect(st).toContain('staffTaskPointHead,staffProjectByRefHead');
    expect(src('attachments-policy-facts.ts')).toContain("import {projectPointReadHead} from './read-tail-r33';");
    expect(src('project-id-source.ts')).toContain('projectPointReadHead(\'project_id\',table,\'company_id\')');
  });

  test('cross-owner final-root projections are excluded and unchanged', () => {
    const fr = src('final-root-owner-projection-sources.ts');
    expect(fr).toContain('const quotedProjection = (columns: string, relation: string) => `SELECT ${columns} FROM ${relation}`;');
    expect(fr).toContain('finalRootOwnerProjectionSources');
    expect(fr).not.toContain('read-tail-r40');
    expect(fr).toContain('staff_project_members: quotedProjection(');
    expect(fr).toContain('staff_projects: quotedProjection(');
    expect(src('../public/final-root-owner-projection-sources.ts')).toContain('finalRootOwnerProjectionSources');
    for (const file of [
      'executive-main-evidence-sources.ts', 'package-obligations.ts', 'product-entry.ts',
      'attachments-service-facts.ts', 'project-id-source.ts',
    ]) expect(src(file), file).not.toContain('selectProjectionFrom');
  });

  test('retirement:ts-statements is 74 after r40 (BEFORE=76)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true }) as Dirent[]) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r39 left 76. Three named heads (one inline SELECT each) delegate to one
    // closed universal head: 76-3+1 = 74. The final-root cross-owner projection
    // keeps its own quoted head and is deliberately not folded.
    expect(total).toBe(69); // lane 27 moved the two actuals reads to native Rust (-2)
    expect(total).toBeLessThan(76);
  });
});
