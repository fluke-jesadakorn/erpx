import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import {
  staffTaskPointHead,
  staffProjectByRefHead,
} from '../../../packages/server-projects/src/adapters/read-tail-r34';

const srcRoot = resolve(__dirname, '../../../packages/server-projects/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Verbatim pre-dedupe inline heads from HEAD before the r34 edit. Byte-identity
// oracles only; they are never live query sources.
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

describe('Projects read-tail r34 staff_tasks point / staff_projects by-ref dedupe', () => {
  test('Family A: one point head reproduces all four former inline heads byte for byte', () => {
    expect(staffTaskPointHead('id,company_id,branch_id,department_id,creator_user_id,assignee_user_id,project_id,template_id,schedule_id,state,version')).toBe(a1);
    expect(staffTaskPointHead("(to_jsonb(t)-'updated_at')::text image", 't')).toBe(a2);
    expect(staffTaskPointHead(a3Columns, 't', 't.')).toBe(a3);
    expect(`${staffTaskPointHead(a4Columns, 't', 't.')} FOR UPDATE OF t`).toBe(a4);
    for (const head of [a1, a2, a3, a4]) {
      expect(head).toContain(' FROM staff_tasks');
      expect(head).toMatch(/WHERE (?:t\.)?id=\? AND (?:t\.)?company_id=\?/);
    }
  });

  test('Family B: one by-ref head reproduces all seven former inline heads byte for byte', () => {
    expect(staffProjectByRefHead('to_jsonb(p)', 'p', 't.project_id', 't.company_id')).toBe(b1);
    expect(staffProjectByRefHead('p.owner_user_id', 'p', 't.project_id', 't.company_id')).toBe(b2);
    expect(`(${staffProjectByRefHead('pr.title', 'pr', 'p.audience_id', 'p.company_id')})`).toBe(b3);
    expect(staffProjectByRefHead('1', 'audience_project', 'p.audience_id', 'p.company_id', '\n      ')).toBe(b4);
    expect(staffProjectByRefHead('1', 'hp', '${task}.project_id', '${task}.company_id', ' ', '\n      ')).toBe(b5);
    expect(staffProjectByRefHead('1', 'hp', 'hs.source_id', '${post}.company_id')).toBe(b6);
    expect(staffProjectByRefHead('1', 'pr', '${alias}.audience_id', '${alias}.company_id')).toBe(b7);
    for (const head of [b1, b2, b3, b4, b5, b6, b7]) {
      expect(head).toContain(' FROM staff_projects ');
      expect(head).toContain('.id=');
      expect(head).toContain('.company_id=');
    }
  });

  test('Family A call sites compose the helper; binds and locks are unchanged', () => {
    const tp = src('task-purchases.ts');
    expect(tp).toContain("import {staffTaskPointHead} from './read-tail-r34';");
    expect(tp).toContain("staffTaskPointHead('id,company_id,branch_id,department_id,creator_user_id,assignee_user_id,project_id,template_id,schedule_id,state,version')}${lock?' FOR UPDATE':''}");
    expect(tp).toContain('${lock?\' FOR UPDATE\':\'\'}');
    expect(tp).toContain('.bind(id,actor.companyId).first<TaskSource>()');
    expect(tp).toContain('staffTaskPointHead("(to_jsonb(t)-\'updated_at\')::text image",\'t\')');
    expect(tp).toContain('.bind(command.id,actor.companyId).first<string>(\'image\')');
    expect(tp).not.toContain('FROM staff_tasks WHERE id=? AND company_id=?${lock?');

    const st = src('scheduled-tasks.ts');
    expect(st).toContain('staffTaskPointHead,staffProjectByRefHead');
    expect(st).toContain("staffProjectByRefHead('to_jsonb(p)','p','t.project_id','t.company_id')");
    expect(st).toContain("staffProjectByRefHead('p.owner_user_id','p','t.project_id','t.company_id')");
    expect(st).toContain("::text image`,'t','t.')}");
    expect(st).toContain("'t','t.')} FOR UPDATE OF t");
    expect(st).not.toContain(`::text image FROM staff_tasks t WHERE t.id=? AND t.company_id=?\`;`);
    expect(st).toContain(' FOR UPDATE OF t`)');
    expect(st).toContain('.bind(target.id,target.companyId).first<{project_id:string|null;version:number;');
    expect(st).toContain('.bind(target.id,target.companyId).first<string>(\'image\')');
    expect(st).toContain('FOR SHARE');
  });

  test('Family B call sites compose the helper; binds, gaps and tails are unchanged', () => {
    const chp = src('collaboration-home-projections.ts');
    expect(chp).toContain("staffProjectByRefHead('pr.title','pr','p.audience_id','p.company_id')");

    const chs = src('collaboration-home-sources.ts');
    expect(chs).toContain("staffProjectByRefHead('1','audience_project','p.audience_id','p.company_id','\\n      ')");
    expect(chs).toContain('projectOwnerOrMemberColumnSql(\'audience_project\',\'social_purchase_actor.user_id\',\'m\')');
    expect(chs).not.toContain('EXISTS(SELECT 1 FROM staff_projects audience_project\n');
    expect(chs).toContain('.bind(...scope.args,id)');

    const prp = src('project-read-projections.ts');
    expect(prp).toContain("staffProjectByRefHead('1','hp',`${task}.project_id`,`${task}.company_id`,' ','\\n      ')");
    expect(prp).toContain("staffProjectByRefHead('1','hp','hs.source_id',`${post}.company_id`)");
    expect(prp).toContain("staffProjectByRefHead('1','pr',`${alias}.audience_id`,`${alias}.company_id`)");
    expect(prp).toContain('AND iam_same_person(hp.owner_user_id,${user},${task}.company_id))`;}');
    expect(prp).toContain("OR ${memberExistsSql('', 'hm', 'hp', 'id', user)})`;}");
    expect(prp).not.toContain('OR EXISTS(SELECT 1 FROM staff_project_members hm WHERE hm.project_id=hp.id');
    expect(prp).toContain("projectOwnerOrMemberColumnSql('pr','a.user_id','pm','id','\\n        ')");
    expect(prp).not.toContain('SELECT 1 FROM staff_projects hp WHERE hp.id=');
    expect(prp).not.toContain('SELECT 1 FROM staff_projects pr WHERE pr.id=');
  });

  test('locks, spaced JOINs and r2-r33 helpers stay untouched', () => {
    for (const [file, symbol] of [
      ['read-tail-r23.ts', 'projectAssignmentAuthorityReadSql'],
      ['read-tail-r27.ts', 'projectMemberExistsSql'],
      ['read-tail-r28.ts', 'assignmentViewerSelect'],
      ['read-tail-r29.ts', 'orderedImageAggregateHead'],
      ['read-tail-r30.ts', 'visibleProjectRowsHead'],
      ['read-tail-r32.ts', 'projectPurchaseSourceHead'],
      ['read-tail-r33.ts', 'projectPointReadHead'],
    ] as const) expect(src(file)).toContain(symbol);
    expect(src('package-obligations.ts')).toContain('LOCK TABLE staff_projects,staff_project_members,staff_project_milestones');
    expect(src('purchases.ts')).toContain('LOCK TABLE staff_projects,staff_project_members IN SHARE ROW EXCLUSIVE MODE');
    expect(src('social-purchase-sources.ts')).toContain('LOCK TABLE staff_projects,staff_project_members IN SHARE MODE');
    expect(src('people-evaluation-facts.ts')).toContain('LOCK TABLE staff_projects,staff_project_members IN SHARE ROW EXCLUSIVE MODE');
    expect(src('project-id-source.ts')).toContain('projectIdSql');
    expect(src('clock-cte.ts')).toContain('export const projectsClockCte=');
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
    // r33 left 105. Family A 4->1 removes 3; Family B 7->1 removes 6: 105-9 = 96.
    expect(total).toBe(67); // lane 27 wave 2 moved the two handoff list/work reads to native (-2) // lane 27 moved the two actuals reads to native Rust (-2) // r37 four-arm fact UNION closed helper (-3) + 3-arm NOT EXISTS proof group (-2): 93->88 // r36 package-actual source + event-fact arms: 95->93 // r35 point-read family merge (projectIdSql -> projectPointReadHead): 96->95 // r38 member EXISTS closed helper (-4) + evidence super-head (-3): 88->81
  });
});
