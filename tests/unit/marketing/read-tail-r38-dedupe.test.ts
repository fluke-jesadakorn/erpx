import { describe, it, expect } from 'vitest';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { formAssetReadHead } from '../../../packages/server-marketing/src/adapters/form-asset-read-head';

const adapter = (name: string) =>
  new URL(`../../../packages/server-marketing/src/adapters/${name}`, import.meta.url);
const src = (name: string) => readFile(adapter(name), 'utf8');
const srcRoot = resolve('packages/server-marketing/src');

// Byte oracles captured from the three original readers before dedupe.
const content = (projection: string) =>
  `SELECT ${projection} FROM marketing_assets WHERE id=? AND company_id=? AND kind='content' AND status='active' AND version=?`;
const formSource =
  "SELECT configuration_json FROM marketing_assets WHERE id=? AND company_id=? AND version=? AND kind='form' AND status='active'";
const formPublic =
  "SELECT id,company_id,name,version,preview_json,configuration_json FROM marketing_assets WHERE id=? AND kind='form' AND status='active'";
const formRow =
  "SELECT id,company_id,version,status,configuration_json FROM marketing_assets WHERE id=? AND company_id=? AND kind='form'";

async function selectTokens(): Promise<number> {
  let total = 0;
  const walk = async (dir: string) => {
    for (const entry of await readdir(dir, { withFileTypes: true })) {
      const path = resolve(dir, entry.name);
      if (entry.isDirectory()) await walk(path);
      else if (entry.name.endsWith('.ts')) total += ((await readFile(path, 'utf8')).match(/SELECT /g) ?? []).length;
    }
  };
  await walk(srcRoot);
  return total;
}

describe('marketing form/content asset read-head dedupe (r38)', () => {
  it('renders all five form/content point reads byte-identically from one head', () => {
    expect(formAssetReadHead({ kind: 'content', projection: '1' })).toBe(content('1'));
    expect(formAssetReadHead({ kind: 'content', projection: 'id,version,preview_json' })).toBe(
      content('id,version,preview_json'),
    );
    expect(formAssetReadHead({ kind: 'form', projection: 'source', company: true, version: true })).toBe(formSource);
    expect(formAssetReadHead({ kind: 'form', projection: 'public' })).toBe(formPublic);
    expect(formAssetReadHead({ kind: 'form-row' })).toBe(formRow);
  });

  it('keeps one SELECT token and one owned marketing_assets source in the head', async () => {
    const head = await src('form-asset-read-head.ts');
    expect(head.match(/SELECT /g)).toHaveLength(1);
    expect(head.match(/FROM marketing_assets/g)).toHaveLength(1);
    expect(head.match(/AND kind='form'/g)).toHaveLength(2);
  });

  it('leaves only delegating wrappers in the three original readers', async () => {
    const contentSource = await src('public-form-content-source.ts');
    expect(contentSource).toContain("formAssetReadHead({ kind: 'content', projection })");
    expect(contentSource).not.toContain('FROM marketing_assets');
    expect(contentSource.match(/SELECT /g)).toBeNull();

    await expect(src('active-form-sql.ts')).rejects.toThrow();

    const purchases = await src('purchases.ts');
    expect(purchases).toContain("formAssetRow:string=formAssetReadHead({kind:'form-row'})");
    expect(purchases).not.toContain(formRow);
  });

  it('preserves binds and every existing consumer of the two public helpers', async () => {
    const publicForm = await src('public-form.ts');
    expect(publicForm).toContain("marketingFormContentSql('1')");
    expect(publicForm).toContain('readMarketingLane40ForCompany(LANE40_FORM_CONTENT_ROW,companyId,');
    expect(publicForm).toContain('nativeMarketingReadFirst<PublicMarketingForm>(MARKETING_RD_FORM_PUBLIC');
    expect(publicForm).not.toContain('.bind(refs.contentAssetId!,companyId,refs.contentVersion!)');
    const marketing = await src('marketing.ts');
    expect(marketing).toContain('readMarketingLane40(LANE40_ACTIVE_FORM_SOURCE,a,');
    expect(marketing).toContain('{assetId:formId,version:formVersion}');
    const purchases = await src('purchases.ts');
    expect(purchases).toContain('database.prepare(formAssetRow).bind(form.id,form.company_id)');
    expect(purchases).toContain('database.prepare(formAssetRow).bind(actual.id,actual.company_id)');
  });

  it('leaves locks, settlement receipts and native read ports untouched', async () => {
    expect(await src('form-asset-lock-sql.ts')).toContain(
      '`SELECT id FROM marketing_assets WHERE id=? AND company_id=?${scope} FOR SHARE`',
    );
    const obligations = await src('package-obligations.ts');
    expect(obligations).toContain("command.operation='marketing.asset.activate'");
    expect(obligations).toContain('LOCK TABLE marketing_assets,marketing_asset_events');
    const purchases = await src('purchases.ts');
    expect(purchases).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(purchases).toContain('IN SHARE ROW EXCLUSIVE MODE');
    expect(await src('native-marketing-read.ts')).toContain('export');
    expect(await src('marketing.ts')).toContain('from "./native-marketing-read"');
  });

  it('retirement:ts-statements KPI stays strictly below BEFORE=64', async () => {
    // r39 lowers the owner tail further (61); this round only pins the drop.
    expect(await selectTokens()).toBeLessThan(64);
  });
});
