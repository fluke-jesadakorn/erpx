import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';

const adapter = (name: string) =>
  new URL(`../../../packages/server-marketing/src/adapters/${name}`, import.meta.url);

describe('marketing active-form point read family', () => {
  it('keeps one fixed active-form read shared by the source-form and public form adapters', async () => {
    const shared = await readFile(adapter('active-form-sql.ts'), 'utf8');
    const head = await readFile(adapter('form-asset-read-head.ts'), 'utf8');
    const source = await readFile(adapter('marketing.ts'), 'utf8');
    const form = await readFile(adapter('public-form.ts'), 'utf8');
    // One canonical declaration; its comment must not smuggle extra query
    // tokens into the owner retirement KPI.
    expect(head.match(/SELECT /g)).toHaveLength(1);
    expect(shared).toContain('formAssetReadHead(');
    expect(source).not.toMatch(/configuration_json FROM marketing_assets WHERE id=\?/);
    expect(form).not.toMatch(/id,company_id,name,version,preview_json,configuration_json FROM marketing_assets/);
    expect(source).toContain("activeFormSql('source',true,true)");
    expect(form).toContain("activeFormSql('public')");
  });

  it('keeps byte-identical output for the versioned and unscoped variants', async () => {
    const { activeFormSql } = await import('../../../packages/server-marketing/src/adapters/active-form-sql.ts');
    expect(activeFormSql('source', true, true)).toBe(
      "SELECT configuration_json FROM marketing_assets WHERE id=? AND company_id=? AND version=? AND kind='form' AND status='active'",
    );
    expect(activeFormSql('public')).toBe(
      "SELECT id,company_id,name,version,preview_json,configuration_json FROM marketing_assets WHERE id=? AND kind='form' AND status='active'",
    );
  });

  it('preserves the bind order of both point reads', async () => {
    const source = await readFile(adapter('marketing.ts'), 'utf8');
    const form = await readFile(adapter('public-form.ts'), 'utf8');
    expect(source).toContain("activeFormSql('source',true,true)).bind(formId,a.companyId,formVersion)");
    expect(form).toContain("activeFormSql('public')).bind(id)");
  });
});
