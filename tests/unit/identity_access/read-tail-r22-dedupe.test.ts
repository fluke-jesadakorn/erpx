import { describe, expect, test } from 'vitest';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { organizationGpoRulesProjection } from '../../../modules/identity-access/server/src/adapters/assignment-projection-source';
import { platformOwnerProjectionSources } from '../../../modules/identity-access/server/src/adapters/platform-owner-projection-sources';
import { peoplePayrollFinalOwnerProjections } from '../../../modules/identity-access/server/src/adapters/people-payroll-final-owner-projections';

const srcRoot = resolve(__dirname, '../../../modules/identity-access/server/src');
const read = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const helperSrc = read('adapters/assignment-projection-source.ts');
const platformSrc = read('adapters/platform-owner-projection-sources.ts');
const payrollSrc = read('adapters/people-payroll-final-owner-projections.ts');
const inline = 'SELECT "id","company_id","target_kind","target_id","version","revision","connection_id","gpo_id","user_filter_group_id","computer_target_ou_id","active","directory_config_status","directory_config_error","legacy_position_id","last_actor_id","last_reason","updated_at" FROM organization_gpo_rules';

describe('Identity access read-tail r22 GPO rules projection family', () => {
  test('renders byte-identical SQL to both pre-dedupe wrapper literals', () => {
    expect(organizationGpoRulesProjection).toBe(inline);
    expect(platformOwnerProjectionSources.organization_gpo_rules).toBe(inline);
    const join = peoplePayrollFinalOwnerProjections().people_lifecycle_organization_gpo_rules_r_join;
    expect(join).toBe(`JOIN (${inline}) r`);
  });

  test('is one owner helper with two callers and no inline literal left behind', () => {
    expect((helperSrc.match(/export const organizationGpoRulesProjection =/g) ?? []).length).toBe(1);
    // import binding + single use at the call site
    expect((platformSrc.match(/\borganizationGpoRulesProjection\b/g) ?? []).length).toBe(2);
    expect((payrollSrc.match(/\borganizationGpoRulesProjection\b/g) ?? []).length).toBe(2);
    expect(platformSrc).not.toContain(inline);
    expect(payrollSrc).not.toContain(inline);
  });

  test('introduces no write or lock and leaves r2-r21 helpers untouched', () => {
    expect(helperSrc).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    expect(read('adapters/account-preferences.ts')).toContain('SELECT id FROM "user" WHERE id=? FOR UPDATE');
    expect(read('adapters/workflow-command-authority.ts')).toContain('FOR SHARE');
    expect(read('adapters/product-sso-store.ts')).toContain('FOR UPDATE');
    for (const h of [
      'roleAllowsSql', 'samePersonSql', 'samePersonQuerySql', 'assignmentActiveQuerySql',
      'memberForShareSql', 'protectedMigrationIdentitySql', 'activeRecipientsSql',
      'idNameByUserIdsSql', 'verifiedUserEmailSql', 'idNameEmailByUserIdsSql', 'publishedRoleSql',
      'grantProbeSql', 'revokedSessionProbeSql', 'sessionByUserSql', 'sessionExistsSql',
    ]) expect((read('adapters/iam-probe-sql.ts').match(new RegExp(`export const ${h} =`, 'g')) ?? []).length, h).toBe(1);
    expect((read('adapters/identity-user-projection-family-r18.ts').match(/export const userAttributeByIdSql =/g) ?? []).length).toBe(1);
    expect((read('adapters/identity-user-projection-family-r21.ts').match(/export const userAttributeByIdExtendedSql =/g) ?? []).length).toBe(1);
  });
});

describe('Identity access read-tail r22 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r22 BEFORE=522 baseline (expected 521)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) {
          total += (readFileSync(p, 'utf8').match(/INSERT INTO|DELETE FROM|UPDATE [a-z_]+ SET|SELECT /g) ?? []).length;
        }
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(522);
    expect(total).toBeLessThanOrEqual(521);
  });
});

describe('Identity access GPO overview native read retirement', () => {
  const statements = (text: string) => (text.match(/INSERT INTO|DELETE FROM|UPDATE [a-z_]+ SET|SELECT /g) ?? []).length;
  // The GPO overview read family: TS authority deleted, native owner port owns it.
  const BEFORE = 605;
  const AFTER = 591;

  test('the TypeScript GPO authority files and their SQL are gone', () => {
    for (const rel of ['adapters/gpo-rules.ts', 'application/gpo-rules.ts', 'public/gpo-rules.ts'])
      expect(existsSync(resolve(srcRoot, rel)), rel).toBe(false);
    expect(statements(read('adapters/native-position-gpo.ts'))).toBe(0);
    expect(read('adapters/native-position-gpo.ts')).not.toMatch(/\bSELECT\b/i);
  });

  test('the native client serves overview, rules projection and not-required', () => {
    const client = read('adapters/native-position-gpo.ts');
    for (const name of [
      'nativePositionGpoOverview',
      'nativePositionGpoOverviewFromHeaders',
      'nativePositionGpoOverviewForHolding',
      'nativeGpoRulesOverview',
      'nativeMarkPositionGpoNotRequired',
    ])
      expect(client, name).toContain(`export ${name.startsWith('nativePositionGpoOverview') ? 'function' : 'async function'} ${name}`);
    expect(client).toContain("'not-required'");
    expect(client).toContain('/technology/position-gpo');
  });

  test('the owner Rust port carries task status and the guarded not-required write', () => {
    const domain = read('domain/position_gpo.rs');
    const adapter = read('adapters/position_gpo_postgres.rs');
    const http = read('adapters/position_gpo_http.rs');
    expect(domain).toContain('pub fn task_status');
    expect(domain).toContain('pub fn present_gpo_task');
    expect(domain).toContain('pub const NOT_REQUIRED_OPERATION');
    expect(adapter).toContain('"tasks": tasks');
    expect(adapter).toContain("disposition='not_required'");
    expect(http).toContain('not-required');
  });

  test('retirement:ts-statements records the GPO family drop and stays retired', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += statements(readFileSync(p, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(BEFORE);
    expect(total).toBeLessThanOrEqual(AFTER);
  });
});
