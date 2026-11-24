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

  it('keeps one canonical release-version guard and one canonical row lock', () => {
    const text = source('adapters/release-acceptance.ts');
    const versionGuard = "EXISTS(SELECT 1 FROM company_release_acceptances WHERE company_id=? AND id=? AND version=? AND revoked_at IS NULL)";
    const rowLock = 'SELECT id FROM company_release_acceptances WHERE company_id=? AND id=? FOR UPDATE';
    expect(text).toContain(`export const RELEASE_VERSION_GUARD_SQL = '${versionGuard}'`);
    expect(text).toContain(`export const RELEASE_ROW_LOCK_SQL = '${rowLock}'`);
    expect(text.match(/RELEASE_VERSION_GUARD_SQL/g)).toHaveLength(4); // declaration + review, revoke, evidence guards
    expect(text.match(/RELEASE_ROW_LOCK_SQL/g)).toHaveLength(3); // declaration + review, revoke locks
    expect(text.match(new RegExp(versionGuard.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'g'))).toHaveLength(1);
    expect(text.match(new RegExp(rowLock.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'g'))).toHaveLength(1);
  });
});
