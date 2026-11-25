import { describe, it, expect, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
vi.mock('server-only', () => ({}));
import { NATIVE_OWNER_WRITE_NOOP_SQL } from '../../../packages/server-manage_erpx/src/adapters/native-owner-write';

const src = new URL('../../../packages/server-manage_erpx/src/', import.meta.url);
const source = (relative: string) => readFileSync(new URL(relative, src), 'utf8');
const callers = [
  'adapters/attachments-finance-facts.ts',
  'adapters/beta-api.ts',
  'adapters/beta-onboarding.ts',
  'adapters/beta-team.ts',
];
const tsSources = (dir: URL): string[] =>
  readdirSync(dir, { withFileTypes: true }).flatMap((entry) =>
    entry.isDirectory()
      ? tsSources(new URL(`${entry.name}/`, dir))
      : entry.name.endsWith('.ts') ? [readFileSync(new URL(entry.name, dir), 'utf8')] : [],
  );

describe('manage_erpx native owner-write no-op dedupe', () => {
  it('keeps exactly one canonical inert no-op SELECT', () => {
    expect(NATIVE_OWNER_WRITE_NOOP_SQL).toBe('SELECT 1');
    expect(NATIVE_OWNER_WRITE_NOOP_SQL.match(/SELECT /g)).toHaveLength(1);
  });

  it('reuses the canonical no-op in every native owner-write caller', () => {
    for (const file of callers) {
      const text = source(file);
      expect(text).toContain('NATIVE_OWNER_WRITE_NOOP_SQL');
      expect(text).not.toContain('db.prepare("SELECT 1")');
    }
  });

  it('lowers the owner TS SELECT count from the recorded BEFORE=23 baseline', () => {
    const total = tsSources(src).join('\n').match(/SELECT /g)?.length ?? 0;
    expect(total).toBe(20);
    expect(total).toBeLessThan(21);
  });
});
