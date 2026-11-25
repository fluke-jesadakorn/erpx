import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';

const adapter = (name: string) =>
  new URL(`../../../packages/server-marketing/src/adapters/${name}`, import.meta.url);

describe('marketing active-form point read family (native lane 40)', () => {
  it('retires the TS active-form builder and routes both point reads to the owner', async () => {
    await expect(readFile(adapter('active-form-sql.ts'), 'utf8')).rejects.toThrow();
    const head = await readFile(adapter('form-asset-read-head.ts'), 'utf8');
    const source = await readFile(adapter('marketing.ts'), 'utf8');
    const form = await readFile(adapter('public-form.ts'), 'utf8');
    expect(head.match(/SELECT /g) ?? []).toHaveLength(0);
    expect(source).not.toMatch(/configuration_json FROM marketing_assets WHERE id=\?/);
    expect(form).not.toMatch(/id,company_id,name,version,preview_json,configuration_json FROM marketing_assets/);
    expect(source).toContain('LANE40_ACTIVE_FORM_SOURCE');
    expect(source).toContain('readMarketingLane40(LANE40_ACTIVE_FORM_SOURCE,a,');
    expect(form).toContain('nativeMarketingReadFirst<PublicMarketingForm>(MARKETING_RD_FORM_PUBLIC');
    expect(form).not.toContain('nativeMarketingReadConfigured');
  });

  it('pins the owner-side byte-identical projections and bind slots', async () => {
    const rust = await readFile(
      new URL('../../../packages/server-marketing/src/adapters/native_lane_40/postgres.rs', import.meta.url),
      'utf8',
    );
    expect(rust).toContain(
      "SELECT (jsonb_build_object('configuration_json',a.configuration_json,'company_id',a.company_id))::text AS value FROM marketing_assets a WHERE a.id=$2 AND a.company_id=$1 AND a.version=$3 AND a.kind='form' AND a.status='active'",
    );
    expect(rust).toContain(
      "SELECT (jsonb_build_object('id',a.id,'version',a.version,'preview_json',a.preview_json,'company_id',a.company_id))::text AS value FROM marketing_assets a WHERE a.id=$2 AND a.company_id=$1 AND a.version=$3 AND a.kind='content' AND a.status='active'",
    );
    const registry = await readFile(
      new URL('../../../packages/server-marketing/src/public/native_read.rs', import.meta.url),
      'utf8',
    );
    expect(registry).toContain(
      "SELECT id,company_id,name,version,preview_json,configuration_json FROM marketing_assets WHERE id=? AND kind='form' AND status='active'",
    );
  });

  it('preserves the asset/version bind order through the native scope', async () => {
    const source = await readFile(adapter('marketing.ts'), 'utf8');
    const form = await readFile(adapter('public-form.ts'), 'utf8');
    expect(source).toContain('{assetId:formId,version:formVersion}');
    expect(form).toContain('{assetId:refs.contentAssetId!,version:refs.contentVersion!}');
    expect(form).toContain('readMarketingLane40ForCompany(LANE40_FORM_CONTENT_ROW,companyId,');
  });
});
