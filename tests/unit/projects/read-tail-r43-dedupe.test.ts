import { describe, expect, test } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { projectDependencyRowsHead } from '../../../packages/server-projects/src/adapters/read-tail-r43';

const srcRoot = resolve(__dirname, '../../../packages/server-projects/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');

// Verbatim BEFORE renders captured at BEFORE=72 from HEAD sources
// b6752f805750dd22 / 67e22732704615fa. Byte-identity oracles only;
// never live query sources. The two witnesses differ in alias, prerequisite
// join and the search copy's third bind; only their closed, bind-free relation
// head is folded here (the whole-witness fold r38 rejected stays rejected).
const PH = '${projectDependencyRowsHead}';
const oracle = {
  head: 'c3348fbe2111f6961de3efcceb6e4df9d03d537496a6ebc13a5edca0e40a6b54',
  searchWitness: '39a40efa92cbb60857619965427edc49d5180e069c376509441900e450583371',
  cohortWitness: '526e0cb8b896231ee08212c58e2ff72897257e8e6236b57a78c0b6bd6a8df577',
} as const;
const render = (s: string) => s.split(PH).join(projectDependencyRowsHead);

describe('Projects read-tail r43 closed dependency-head family', () => {
  test('one closed head renders the shared milestone-dependency relation byte for byte', () => {
    expect(projectDependencyRowsHead).toBe('SELECT 1 FROM staff_project_milestone_dependencies ');
    expect(sha256(projectDependencyRowsHead)).toBe(oracle.head);
    expect(Buffer.byteLength(projectDependencyRowsHead)).toBe(51);
    // The head is bind-free: bind order/count of both callers is unchanged.
    expect(projectDependencyRowsHead.match(/\?/g)).toBeNull();
    const helper = src('read-tail-r43.ts');
    expect(helper.match(/SELECT /g)).toHaveLength(1);
    expect(helper.match(/export const projectDependencyRowsHead/g)).toHaveLength(1);
    expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  test('both executive sites delegate to the one head and keep BEFORE bytes', () => {
    const ev = src('executive-main-evidence-sources.ts');
    const co = src('executive-milestone-cohort.ts');
    for (const s of [ev, co]) {
      expect(s).toContain("import { projectDependencyRowsHead } from './read-tail-r43';");
      expect(s.match(/\$\{projectDependencyRowsHead\}/g)).toHaveLength(1);
      // The former inline literal is gone; only the renderer carries it.
      expect(s).not.toContain('SELECT 1 FROM staff_project_milestone_dependencies ');
    }

    // Search-probe EXISTS witness: reconstruct the BEFORE render from the head + unchanged tail.
    const sa = ev.indexOf('EXISTS(\n      ' + PH + 'search_dependency');
    expect(sa).toBeGreaterThan(-1);
    const endSearch = 'position(lower(?) in lower(prerequisite.title))>0)';
    const sea = ev.indexOf(endSearch, sa) + endSearch.length;
    const searchRendered = render(ev.slice(sa, sea));
    expect(sha256(searchRendered)).toBe(oracle.searchWitness);
    expect(Buffer.byteLength(searchRendered)).toBe(442);
    expect(searchRendered.match(/\?/g)).toHaveLength(1);
    expect(searchRendered).toContain("prerequisite.state<>'completed'");

    // Blocker-cohort EXISTS witness: fixed aliases keep the BEFORE bytes.
    const ca = co.indexOf('AND EXISTS(' + PH + 'dependency');
    expect(ca).toBeGreaterThan(-1);
    const endCohort = "prerequisite.state<>'completed')";
    const cea = co.indexOf(endCohort, ca) + endCohort.length;
    const cohortRendered = render(co.slice(ca, cea));
    expect(sha256(cohortRendered)).toBe(oracle.cohortWitness);
    expect(Buffer.byteLength(cohortRendered)).toBe(487);
    expect(cohortRendered.match(/\?/g)).toBeNull();
    expect(cohortRendered).toContain('JOIN staff_project_milestones prerequisite');
  });

  test('locks, native SQL, cross-owner projections and prior helpers stay untouched', () => {
    const obl = src('package-obligations.ts');
    expect(obl).toContain('LOCK TABLE staff_projects,staff_project_members,staff_project_milestones');
    const st = src('scheduled-tasks.ts');
    expect(st).toContain('SELECT id FROM staff_projects WHERE id=? AND company_id=? FOR SHARE');
    expect(st).toContain('FOR UPDATE OF t');
    expect(src('purchases.ts')).toContain('LOCK TABLE staff_projects,staff_project_members IN SHARE ROW EXCLUSIVE MODE');
    expect(src('social-purchase-sources.ts')).toContain('LOCK TABLE staff_projects,staff_project_members IN SHARE MODE');
    // Cross-owner / native projections keep their own literals and do not import r43.
    expect(src('final-root-owner-projection-sources.ts')).toContain(
      'const quotedProjection = (columns: string, relation: string) => `SELECT ${columns} FROM ${relation}`;');
    expect(src('final-root-owner-projection-sources.ts')).not.toContain('read-tail-r43');
    for (const f of ['project_actuals.rs', 'project_attachment.rs', 'project_graph_read.rs']) {
      expect(readFileSync(resolve(srcRoot, 'adapters', f), 'utf8')).not.toContain('projectDependencyRowsHead');
    }
    // Prior r23-r42 head families keep their own definitions.
    for (const [file, symbol] of [
      ['read-tail-r23.ts', 'projectAssignmentAuthorityReadSql'],
      ['read-tail-r30.ts', 'visibleProjectRowsHead'],
      ['read-tail-r37.ts', 'notExistsRow'],
      ['read-tail-r38.ts', 'memberExistsSql'],
      ['read-tail-r41.ts', 'clockNowMsHead'],
    ] as const) expect(src(file)).toContain(symbol);
  });

  test('retirement:ts-statements is 71 after r43 (BEFORE=72)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true }) as Dirent[]) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r42 left 72. The two byte-identical milestone-dependency relation heads
    // (one per executive witness) now share one closed, bind-free head:
    // 72-2+1 = 71. The witness tails, locks, native SQL and cross-owner
    // projections keep their own literals and are not folded.
    expect(total).toBe(71);
    expect(total).toBeLessThan(72);
  });
});
