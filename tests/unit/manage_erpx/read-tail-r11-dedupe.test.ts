import { describe, it, expect, vi } from 'vitest';
import { readFile, readdir } from 'node:fs/promises';

vi.mock('server-only', () => ({}));

const adapter = (name: string) =>
  new URL(`../../../packages/server-manage_erpx/src/adapters/${name}`, import.meta.url);
const srcDir = new URL('../../../packages/server-manage_erpx/src/', import.meta.url);

const OLD_ACTIVE_SLOT = "SELECT 1 FROM beta_slots WHERE holding_id=? AND state='active'";
const OLD_ONBOARDING_SLOT =
  'SELECT slug,company_name,branch_name,state FROM beta_slots WHERE owner_user_id=?';

async function tsSources(dir: URL): Promise<string[]> {
  const out: string[] = [];
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const url = new URL(`${entry.name}${entry.isDirectory() ? '/' : ''}`, dir);
    if (entry.isDirectory()) out.push(...(await tsSources(url)));
    else if (entry.name.endsWith('.ts')) out.push(await readFile(url, 'utf8'));
  }
  return out;
}

describe('manage_erpx beta_slots point-read native cutover (r8)', () => {
  it('keeps the retired byte-identical statements only in the Rust registry', async () => {
    const rust = await readFile(adapter('native_owner_read_sql.rs'), 'utf8');
    expect(rust).toContain(`"${OLD_ACTIVE_SLOT}"`);
    expect(rust).toContain(`"${OLD_ONBOARDING_SLOT}"`);
    expect(rust).toContain('ME_RD_SLOT_ACTIVE');
    expect(rust).toContain('ME_RD_SLOT_OWNER');
  });

  it('cuts both owner callers to the native client without emitted SQL', async () => {
    const active = await readFile(adapter('runtime-active-slot-source.ts'), 'utf8');
    const entry = await readFile(adapter('people-app-entrypoint-facts.ts'), 'utf8');
    expect(active).toContain('nativeBetaSlotActive(db, holdingId)');
    expect(entry).toContain('nativeOwnerOnboardingSlot(db,userId)');
    for (const text of [active, entry]) {
      expect(text).not.toContain('betaSlotPointSql');
      expect(text).not.toMatch(/SELECT .* FROM beta_slots WHERE /);
    }
  });

  it('lowers the owner TS SELECT count from the recorded BEFORE=26 baseline', async () => {
    const total = (await tsSources(srcDir)).join('\n').match(/SELECT /g)?.length ?? 0;
    expect(total).toBe(15);
    expect(total).toBeLessThan(26);
  });
});

const OLD_BOUND_COUNT =
  'SELECT count(*) FROM beta_members m JOIN hr_people p ON p.user_id=m.user_id WHERE p.company_id=?';
const OLD_POSITIONAL_COUNT =
  'SELECT count(*) FROM beta_members m JOIN hr_people p ON p.user_id=m.user_id WHERE p.company_id=$1';

describe('manage_erpx beta_members count SELECT-head family (r12)', () => {
  it('emits byte-identical SQL for both admitted dialect call shapes', async () => {
    const { betaMemberCountSql } = await import(
      '../../../packages/server-manage_erpx/src/adapters/beta-member-count-sql'
    );
    const { betaManagementPeopleSources } = await import(
      '@erpx/server-people_payroll/public/beta-management-sources'
    );
    const sources = betaManagementPeopleSources({} as never);
    expect(betaMemberCountSql(sources, 'bound')).toBe(OLD_BOUND_COUNT);
    expect(betaMemberCountSql(sources, 'positional')).toBe(OLD_POSITIONAL_COUNT);
  });

  it('keeps one SELECT-head declaration and reuses it at both owner call sites', async () => {
    const helper = await readFile(adapter('beta-member-count-sql.ts'), 'utf8');
    expect(helper.match(/SELECT count\(\*\) FROM beta_members m /g)).toHaveLength(1);
    const team = await readFile(adapter('beta-team.ts'), 'utf8');
    expect(team.split('betaMemberCountSql(').length - 1).toBe(2);
    expect(team).not.toMatch(/SELECT count\(\*\) FROM beta_members m/);
  });

  it('leaves locks, native authority and beta-policy sources untouched', async () => {
    const untouched = [
      'company-provisioning-attachment-read-sources.ts',
      'native-owner-write.ts',
      'beta-policy-point-sql.ts',
      'records-ai-limit-projection.ts',
      'identity-snapshot-sources.ts',
      'attachments-service-facts.ts',
    ];
    for (const file of untouched) {
      const text = await readFile(adapter(file), 'utf8');
      expect(text).not.toContain('betaMemberCountSql');
    }
    const team = await readFile(adapter('beta-team.ts'), 'utf8');
    expect(team).toContain("${betaPolicyPointSql('member_limit')}");
  });

  it('lowers the owner TS SELECT count from the recorded BEFORE=25 baseline', async () => {
    const total = (await tsSources(srcDir)).join('\n').match(/SELECT /g)?.length ?? 0;
    expect(total).toBe(15);
    expect(total).toBeLessThan(25);
  });
});

const OLD_BOOTSTRAP_SCOPE = 'SELECT 1 FROM beta_bootstrap h WHERE h.singleton=1 AND h.holding_id=?';
const OLD_BOOTSTRAP_BINDING = 'SELECT holding_id FROM beta_bootstrap WHERE singleton=1';

describe('manage_erpx beta_bootstrap point-read parameterized family (r13)', () => {
  it('emits byte-identical SQL for both admitted projection call shapes', async () => {
    const { betaBootstrapPointSql } = await import(
      '../../../packages/server-manage_erpx/src/adapters/beta-bootstrap-point-sql'
    );
    expect(betaBootstrapPointSql('1', 'h', ' AND h.holding_id=?')).toBe(OLD_BOOTSTRAP_SCOPE);
    expect(betaBootstrapPointSql('holding_id')).toBe(OLD_BOOTSTRAP_BINDING);
  });

  it('keeps one SELECT-head declaration and reuses it at both owner call sites', async () => {
    const helper = await readFile(adapter('beta-bootstrap-point-sql.ts'), 'utf8');
    expect(helper.match(/SELECT \$\{projection\} FROM beta_bootstrap/g)).toHaveLength(1);
    const src = await readFile(adapter('ai-gateway-sources.ts'), 'utf8');
    expect(src.split('betaBootstrapPointSql(').length - 1).toBe(2);
    expect(src).not.toMatch(/SELECT .* FROM beta_bootstrap/);
  });

  it('leaves locks, native authority and beta-policy sources untouched', async () => {
    const untouched = [
      'company-provisioning-attachment-read-sources.ts',
      'native-owner-write.ts',
      'beta-policy-point-sql.ts',
      'records-ai-limit-projection.ts',
      'identity-snapshot-sources.ts',
      'attachments-service-facts.ts',
    ];
    for (const file of untouched) {
      const text = await readFile(adapter(file), 'utf8');
      expect(text).not.toContain('betaBootstrapPointSql');
    }
  });

  it('lowers the owner TS SELECT count from the recorded BEFORE=24 baseline', async () => {
    const total = (await tsSources(srcDir)).join('\n').match(/SELECT /g)?.length ?? 0;
    expect(total).toBe(15);
    expect(total).toBeLessThan(24);
  });
});
