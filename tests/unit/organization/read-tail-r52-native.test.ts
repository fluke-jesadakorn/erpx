import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';

const srcRoot = resolve(__dirname, '../../../packages/server-organization/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const org = read('organization.ts');
const registry = read('organization_owner_write.rs');

// The two retired owner-local list reads. In the pre-r52 (BEFORE=226) source
// they were inline `db.prepare(...).bind(companyId).all()` statements on the
// `levels` and `templates` ports of `portsFor`. r52 routes them through the
// native Organization owner registry; the SQL moves verbatim into the Rust
// kind table and the TypeScript caller sends only a typed kind and the
// Company positional value. Row shapes and fail-closed behavior are unchanged.
const LEVELS_KIND = 'org_access_read_levels';
const TEMPLATES_KIND = 'org_access_read_templates';
const LEVELS_SQL = 'SELECT id,name,level_number FROM staff_levels WHERE company_id=? AND active=1 AND level_number IS NOT NULL ORDER BY level_number';
const TEMPLATES_SQL = 'SELECT id,name,version,positions_json FROM organization_templates WHERE company_id=? ORDER BY name';
const LEVELS_CALL = `nativeOrgAll<{id:string;name:string;level_number:number}>(db,companyId,'${LEVELS_KIND}',[companyId])`;
const TEMPLATES_CALL = `nativeOrgAll<{id:string;name:string;version:number;positions_json:string}>(db,companyId,'${TEMPLATES_KIND}',[companyId])`;

describe('Organization read-tail r52 native cutover', () => {
  test('kind pins: both ports send typed native kinds declared by the Rust registry', () => {
    expect(org).toContain(LEVELS_CALL);
    expect(org).toContain(TEMPLATES_CALL);
    expect(org.split('nativeOrgAll<').length - 1).toBe(2);
    expect(org).toContain("import { nativeOrgAll, nativeOrgStatement } from './native-organization-owner-write';");

    expect(registry).toContain(`pub const ORG_ACCESS_READ_LEVELS: &str = "${LEVELS_KIND}";`);
    expect(registry).toContain(`pub const ORG_ACCESS_READ_TEMPLATES: &str = "${TEMPLATES_KIND}";`);
    expect(registry).toContain('    ORG_ACCESS_READ_LEVELS,\n    ORG_ACCESS_READ_TEMPLATES,');
  });

  test('bind pins: each call sends only the Company positional value', () => {
    // The exact call strings pin the single `[companyId]` bind; no extra scope
    // value and no local `.bind(companyId)` on either retired read remains.
    expect(org).toContain(`'${LEVELS_KIND}',[companyId])`);
    expect(org).toContain(`'${TEMPLATES_KIND}',[companyId])`);
    expect(org).not.toContain(`'${LEVELS_KIND}',[companyId,`);
    expect(org).not.toContain(`'${TEMPLATES_KIND}',[companyId,`);
    expect(org).not.toContain(`${LEVELS_SQL}).bind(companyId)`);
    expect(org).not.toContain(`${TEMPLATES_SQL}).bind(companyId)`);
  });

  test('byte pins: retired literals move verbatim into the Rust kind table', () => {
    expect(org).not.toContain(LEVELS_SQL);
    expect(org).not.toContain(TEMPLATES_SQL);
    expect(registry).toContain(`"${LEVELS_SQL}"`);
    expect(registry).toContain(`"${TEMPLATES_SQL}"`);
    expect(registry).toContain('ORG_ACCESS_READ_TEMPLATES => {');

    // Locks/advisory/version guards/spaced joins and native writes stay untouched.
    expect(org.split('nativeOrgStatement(').length - 1).toBe(15);
    expect(org).toContain("EXISTS(SELECT 1 FROM authority_seats WHERE company_id=? AND kind='head' AND scope_id=? AND version=?");
    expect(read('project-worker-startup-sources.ts')).toContain('LOCK TABLE organization_nodes,organization_positions,staff_levels,authority_seats IN SHARE MODE');
    expect(read('pms-command-facts.ts')).toContain('FOR SHARE');
    expect(read('organization-nodes-spaced-projections.ts')).toContain('"id", "kind", "slug"');
  });

  test('retirement:ts-statements falls from 226 to 224 after the two read cutovers', () => {
    let space = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) space += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // Reusing org_access_read_levels (-1) and adding org_access_read_templates
    // (-1) removes both inline list SELECTs from organization.ts.
    expect(space).toBe(216);
    expect(space).toBeLessThan(226);
  });
});
