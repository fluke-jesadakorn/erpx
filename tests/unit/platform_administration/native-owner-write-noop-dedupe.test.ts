import { describe, it, expect, vi } from 'vitest';
import { readFileSync } from 'node:fs';
vi.mock('server-only', () => ({}));
import { NATIVE_OWNER_WRITE_NOOP_SQL } from '../../../packages/server-platform_administration/src/adapters/native-owner-write';

const source = (relative: string) =>
  readFileSync(new URL(`../../../packages/server-platform_administration/src/${relative}`, import.meta.url), 'utf8');

describe('platform administration native owner-write no-op dedupe', () => {
  it('keeps exactly one canonical inert no-op SELECT', () => {
    expect(NATIVE_OWNER_WRITE_NOOP_SQL).toBe('SELECT 1');
    expect(NATIVE_OWNER_WRITE_NOOP_SQL.match(/SELECT /g)).toHaveLength(1);
  });

  it('reuses the canonical no-op in every native owner-write caller', () => {
    for (const file of ['adapters/release-acceptance.ts', 'adapters/attachments-finance-facts.ts']) {
      const text = source(file);
      expect(text).toContain('NATIVE_OWNER_WRITE_NOOP_SQL');
      expect(text).not.toContain('db.prepare("SELECT 1")');
    }
  });
});
