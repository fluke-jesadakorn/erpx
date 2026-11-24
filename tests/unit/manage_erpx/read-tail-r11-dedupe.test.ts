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

describe('manage_erpx beta_slots point-read parameterized family', () => {
  it('emits byte-identical SQL for both admitted call shapes', async () => {
    const { betaSlotPointSql } = await import(
      '../../../packages/server-manage_erpx/src/adapters/beta-slot-point-sql'
    );
    expect(betaSlotPointSql('1', 'holding_id', " AND state='active'")).toBe(OLD_ACTIVE_SLOT);
    expect(betaSlotPointSql('slug,company_name,branch_name,state', 'owner_user_id')).toBe(
      OLD_ONBOARDING_SLOT,
    );
  });

  it('keeps one point-read declaration and reuses it at both owner call sites', async () => {
    const helper = await readFile(adapter('beta-slot-point-sql.ts'), 'utf8');
    expect(helper.match(/SELECT \$\{projection\} FROM beta_slots WHERE \$\{key\}=\?/g)).toHaveLength(1);
    for (const file of ['runtime-active-slot-source.ts', 'people-app-entrypoint-facts.ts']) {
      const text = await readFile(adapter(file), 'utf8');
      expect(text).toContain('betaSlotPointSql(');
      expect(text).not.toMatch(/SELECT .* FROM beta_slots WHERE /);
    }
  });

  it('preserves the exact prepared SQL for both existing callers', async () => {
    vi.resetModules();
    vi.doMock('server-only', () => ({}));
    const { runtimeBetaSlotActive } = await import(
      '../../../packages/server-manage_erpx/src/adapters/runtime-active-slot-source'
    );
    const { ownerOnboardingSlot } = await import(
      '../../../packages/server-manage_erpx/src/adapters/people-app-entrypoint-facts'
    );
    const prepared: string[] = [];
    const db = {
      prepare(sql: string) {
        prepared.push(sql);
        return {
          bind() {
            return {
              first: async () => ({ ok: 1 }),
            };
          },
          first: async () => ({ ok: 1 }),
        };
      },
    };
    expect(await runtimeBetaSlotActive(db as never, 'h001')).toBe(true);
    await ownerOnboardingSlot(db as never, 'u001');
    expect(prepared).toEqual([OLD_ACTIVE_SLOT, OLD_ONBOARDING_SLOT]);
  });

  it('lowers the owner TS SELECT count from the recorded BEFORE=26 baseline', async () => {
    const total = (await tsSources(srcDir)).join('\n').match(/SELECT /g)?.length ?? 0;
    expect(total).toBe(25);
    expect(total).toBeLessThan(26);
  });
});
