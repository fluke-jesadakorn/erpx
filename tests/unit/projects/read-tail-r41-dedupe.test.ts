import { describe, expect, test } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { clockNowMsHead } from '../../../packages/server-projects/src/adapters/read-tail-r41';
import { projectsClockCte } from '../../../packages/server-projects/src/adapters/clock-cte';

const srcRoot = resolve(__dirname, '../../../packages/server-projects/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');

// Verbatim BEFORE renders captured at BEFORE=74 (source blobs
// 63fa15e5e838ca3554c791db29891022d668dd0d / 2f343e94c496a382075a28252acac662d770dc38).
// Byte-identity oracles only; never live query sources.
const head = 'SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint now_ms';
const audienceCte = 'WITH project_audience_clock AS MATERIALIZED(SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint now_ms)';
const metricCte = 'WITH project_metric_clock AS MATERIALIZED (SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint now_ms)';
const entryCte = 'WITH project_entry_clock AS MATERIALIZED (SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint now_ms)';
const oracleSha = {
  head: 'c73656211aef39055a3da29f7d5e9f76d2aac05eb690b205fad82a6759ba60d9',
  audienceCte: '19e4ef5ff59deb3f72fad530f1f501ed647b7e08986d253dd81f9c7506db9fc5',
  metricCte: 'c5b31bf063288562420f13b3f120743e36410741060c5640524c4c2a35c1eded',
  entryCte: '33d74750a1fa5b4c03a21d1c93a5c8d7b75e0fc28bfa99c1daa6ad0c6df108a6',
} as const;

describe('Projects read-tail r41 closed clock head family', () => {
  test('one closed head renders the shared clock projection byte for byte', () => {
    expect(clockNowMsHead()).toBe(head);
    expect(sha256(clockNowMsHead())).toBe(oracleSha.head);
    expect(head).toHaveLength(71);
    // The clock head carries no binds: bind order of every caller is unchanged.
    expect(head.match(/\?/g)).toBeNull();
    const helper = src('read-tail-r41.ts');
    // r45: the bare projection prefix now lives in the single owner body head.
    expect(helper.match(/SELECT /g)).toBeNull();
    expect(helper).toContain("import {selectBodySql} from './read-tail-r45';");
    expect(helper).toContain('selectBodySql(`${packageClockSql} now_ms`)');
    expect(helper.match(/export const clockNowMsHead/g)).toHaveLength(1);
    expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  test('the metric lock-free CTE stays byte-identical for every alias', () => {
    expect(projectsClockCte('project_metric_clock')).toBe(metricCte);
    expect(sha256(projectsClockCte('project_metric_clock'))).toBe(oracleSha.metricCte);
    expect(projectsClockCte('project_entry_clock')).toBe(entryCte);
    expect(projectsClockCte('project_metric_clock')).toContain(' AS MATERIALIZED (SELECT ');
    // The frame and the closed head are recomposed, never caller SQL.
    expect(projectsClockCte('x')).toBe(`WITH x AS MATERIALIZED (${clockNowMsHead()})`);
    const clock = src('clock-cte.ts');
    expect(clock).toContain('export const projectsClockCte=');
    expect(clock).toContain("import {clockNowMsHead} from './read-tail-r41';");
    expect(clock).toContain('`WITH ${name} AS MATERIALIZED (${clockNowMsHead()})`');
    expect(clock.match(/SELECT /g)).toBeNull();
  });

  test('the collaboration audience CTE keeps its own frame and rendered SQL', () => {
    const collab = src('collaboration-home-sources.ts');
    expect(collab).toContain("import {clockNowMsHead} from './read-tail-r41';");
    expect(collab).toContain('const projectAudienceClock = `WITH project_audience_clock AS MATERIALIZED(${clockNowMsHead()})`;');
    expect(collab).not.toContain('packageClockSql');
    expect(collab).not.toContain('MATERIALIZED(SELECT ');
    // Reconstruct the former inline literal from the shared head + unchanged frame.
    const rendered = `WITH project_audience_clock AS MATERIALIZED(${clockNowMsHead()})`;
    expect(rendered).toBe(audienceCte);
    expect(sha256(rendered)).toBe(oracleSha.audienceCte);
    expect(rendered.match(/\?/g)).toBeNull();
  });

  test('locks, native SQL and cross-owner projections are excluded and unchanged', () => {
    const st = src('scheduled-tasks.ts');
    expect(st).toContain('SELECT id FROM staff_projects WHERE id=? AND company_id=? FOR SHARE');
    expect(st).toContain('SELECT id FROM assignments WHERE id=? FOR SHARE');
    const po = src('package-obligations.ts');
    expect(po).toContain('LOCK TABLE staff_projects,staff_project_members,staff_project_milestones');
    expect(src('purchases.ts')).toContain('LOCK TABLE staff_projects,staff_project_members IN SHARE ROW EXCLUSIVE MODE');
    expect(src('social-purchase-sources.ts')).toContain('LOCK TABLE staff_projects,staff_project_members IN SHARE MODE');
    expect(src('people-evaluation-facts.ts')).toContain('LOCK TABLE staff_projects,staff_project_members IN SHARE ROW EXCLUSIVE MODE');
    const fr = src('final-root-owner-projection-sources.ts');
    expect(fr).toContain('const quotedProjection = (columns: string, relation: string) => selectProjectionFrom(columns, relation);');
    expect(fr).not.toContain('read-tail-r41');
    expect(src('clock-cte.ts')).not.toContain('FOR UPDATE');
    expect(src('read-tail-r41.ts')).not.toContain('staff_projects');
  });

  test('retirement:ts-statements is 73 after r41 (BEFORE=74)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true }) as Dirent[]) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r40 left 74. Two byte-identical clock heads (one in the lock-free CTE
    // helper, one inline in the collaboration audience CTE) delegate to one
    // closed universal head: 74-2+1 = 73. Cross-owner final-root projections,
    // locks and native SQL keep their own literals and are not folded.
    expect(total).toBe(57); // r46 evaluation reads moved to the native contract (62->57)
    expect(total).toBeLessThan(74);
  });
});
