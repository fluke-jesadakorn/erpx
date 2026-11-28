import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';

const adapter = (name: string) =>
  new URL(`../../../modules/marketing/server/src/adapters/${name}`, import.meta.url);

describe('marketing FOR SHARE form-asset lock native read family', () => {
  it('deletes the TS lock head and appends the lock tail on the caller', async () => {
    await expect(readFile(adapter('form-asset-lock-sql.ts'), 'utf8')).rejects.toThrow();
    const form = await readFile(adapter('public-form.ts'), 'utf8');
    const submission = await readFile(adapter('public-submission.ts'), 'utf8');
    expect(form).toContain('nativeMarketingPlan(MARKETING_RD_FORM_LOCK)');
    expect(submission).toContain('nativeMarketingPlan(MARKETING_RD_FORM_LOCK_VERSIONED)');
    expect(form).toContain('`${lock.sql} FOR SHARE`');
    expect(submission).toContain('`${lock.sql} FOR SHARE`');
    expect(form).not.toMatch(/FROM marketing_assets WHERE id=\? AND company_id=\?/);
    expect(submission).not.toMatch(/AND version=\? AND status='active' FOR SHARE/);
  });

  it('registers both closed lock kinds on the native transport', async () => {
    const transport = await readFile(adapter('native-marketing-read.ts'), 'utf8');
    expect(transport).toContain("export const MARKETING_RD_FORM_LOCK = 'form-asset-lock';");
    expect(transport).toContain("export const MARKETING_RD_FORM_LOCK_VERSIONED = 'form-asset-lock-versioned';");
    expect(transport).toContain('  MARKETING_RD_FORM_LOCK,\n  MARKETING_RD_FORM_LOCK_VERSIONED,');
  });

  it('preserves the bind order of both lock reads', async () => {
    const form = await readFile(adapter('public-form.ts'), 'utf8');
    const submission = await readFile(adapter('public-submission.ts'), 'utf8');
    expect(form).toContain('`${lock.sql} FOR SHARE`).bind(refs.contentAssetId,companyId)');
    expect(submission).toContain('`${lock.sql} FOR SHARE`).bind(row.id,row.company_id,row.version)');
  });
});
