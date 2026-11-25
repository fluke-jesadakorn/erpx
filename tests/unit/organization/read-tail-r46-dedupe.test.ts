import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';

const srcRoot = resolve(__dirname, '../../../packages/server-organization/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const source = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');

const APPLIED = "SELECT snapshot_json FROM organization_node_change_events WHERE change_id=? AND action='applied' ORDER BY created_at DESC LIMIT 1";
const SUBMITTED = "SELECT snapshot_json FROM organization_node_change_events WHERE change_id=? AND action='submitted' ORDER BY created_at DESC,id DESC LIMIT 1";

describe('Organization change-event snapshot family retired to native owner', () => {
  test('shared r46 helper is deleted and both former consumers send typed kinds', () => {
    expect(() => read('organization-read-tail-sources-r46.ts')).toThrow();
    const drafts = read('organization-drafts.ts');
    const org = read('organization.ts');
    for (const file of [drafts, org]) expect(file).not.toContain('snapshotJsonByAction');
    expect(drafts).toContain("nativeOrgAll<{snapshot_json:string}>(db,companyId,'change_event_snapshot_applied',[id])");
    expect(org).toContain("nativeOrgAll<{snapshot_json:string}>(db,companyId,'change_event_snapshot_submitted',[id])");
    expect(drafts).not.toContain(APPLIED);
    expect(org).not.toContain(SUBMITTED);
  });

  test('applications pass the authoritative Company to both port reads', () => {
    const drafts = source('application/organization-drafts.ts');
    expect(drafts).toContain('appliedMapping(companyId:string,changeId:string)');
    expect(drafts).toContain('ports.appliedMapping(actor.companyId,request.change_id!)');
    expect(drafts).toContain('p.appliedMapping(row.company_id,dep.changeId)');
    const org = source('application/organization.ts');
    expect(org).toContain('submittedSnapshot(companyId:string,id:string)');
    expect(org).toContain('ports.submittedSnapshot(actor.companyId,id)');
  });

  test('Rust owner holds both byte-identical read-only statements', () => {
    const registry = read('organization_owner_write.rs');
    for (const [kind, sql, sha] of [
      ['change_event_snapshot_applied', APPLIED, 'c2440b239c1bd6e293774e4b15ebc6c9515966a4b787401fd5a3bc995f4cd880'],
      ['change_event_snapshot_submitted', SUBMITTED, 'cf2d32fbb2e088beaddb7b0ada30f1f82ecfd2a3da5a261594ebcafbf15f3e7c'],
    ]) {
      expect(registry).toContain(`"${sql}"`);
      expect(registry).toContain(`"${sha}"`);
      expect(sql.toUpperCase().startsWith('SELECT ')).toBe(true);
      expect(sql).not.toMatch(/INSERT|UPDATE|DELETE|FOR UPDATE|FOR SHARE|LOCK TABLE/);
    }
    expect(registry).toContain('CHANGE_EVENT_SNAPSHOT_APPLIED: &str = "change_event_snapshot_applied";');
    expect(registry).toContain('CHANGE_EVENT_SNAPSHOT_SUBMITTED: &str = "change_event_snapshot_submitted";');
  });

  test('locks and spaced JOIN constants stay byte-identical', () => {
    expect(read('project-worker-startup-sources.ts')).toContain('LOCK TABLE organization_nodes,organization_positions,staff_levels,authority_seats IN SHARE MODE');
    expect(read('pms-command-facts.ts')).toContain('FOR SHARE');
    for (const f of ['people-lifecycle-facts.ts', 'organization-drafts.ts', 'pos-core-facts.ts', 'finance-starter-sources.ts']) {
      expect(read(f)).toContain('FOR UPDATE');
    }
    expect(read('organization-nodes-spaced-projections.ts')).toContain('"id", "kind", "slug"');
  });

  test('retirement:ts-statements drops to 186 after deleting the shared helper literal', () => {
    let space = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) space += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(space).toBe(178);
    expect(space).toBeLessThan(240);
  });
});
