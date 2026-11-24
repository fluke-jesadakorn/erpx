import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';

const adapter = (name: string) =>
  new URL(`../../../packages/server-marketing/src/adapters/${name}`, import.meta.url);

describe('marketing FOR SHARE form-asset lock read family', () => {
  it('keeps one fixed lock read shared by the public form and submission adapters', async () => {
    const shared = await readFile(adapter('form-asset-lock-sql.ts'), 'utf8');
    const form = await readFile(adapter('public-form.ts'), 'utf8');
    const submission = await readFile(adapter('public-submission.ts'), 'utf8');
    // One canonical declaration; its comment must not smuggle extra query
    // tokens into the owner retirement KPI.
    expect(shared.match(/SELECT /g)).toHaveLength(1);
    expect(form).not.toMatch(/FROM marketing_assets WHERE id=\? AND company_id=\? FOR SHARE/);
    expect(submission).not.toMatch(/AND version=\? AND status='active' FOR SHARE/);
    expect(form).toContain('db.prepare(formAssetLockSql(false))');
    expect(submission).toContain('db.prepare(formAssetLockSql(true))');
  });

  it('keeps FOR SHARE as the final clause with byte-identical output', async () => {
    const { formAssetLockSql } = await import('../../../packages/server-marketing/src/adapters/form-asset-lock-sql.ts');
    // Both variants share one base read; only the closed version/status scope
    // differs. Lock mode and placement stay fixed in the single template.
    expect(formAssetLockSql(false)).toBe(
      'SELECT id FROM marketing_assets WHERE id=? AND company_id=? FOR SHARE',
    );
    expect(formAssetLockSql(true)).toBe(
      "SELECT id FROM marketing_assets WHERE id=? AND company_id=? AND version=? AND status='active' FOR SHARE",
    );
  });

  it('preserves the bind order of both lock reads', async () => {
    const form = await readFile(adapter('public-form.ts'), 'utf8');
    const submission = await readFile(adapter('public-submission.ts'), 'utf8');
    expect(form).toContain('formAssetLockSql(false)).bind(refs.contentAssetId,companyId)');
    expect(submission).toContain('formAssetLockSql(true)).bind(row.id,row.company_id,row.version)');
  });
});
