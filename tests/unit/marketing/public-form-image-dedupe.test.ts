import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';

const adapter = (name: string) =>
  new URL(`../../../packages/server-marketing/src/adapters/${name}`, import.meta.url);

const form =
  "SELECT company_id,status,configuration_json,to_jsonb(form)::text image FROM marketing_assets form WHERE id=? AND kind='form'";
const content =
  "SELECT to_jsonb(content)::text image FROM marketing_assets content WHERE id=? AND company_id=? AND kind='content'";
const receipt = `SELECT jsonb_build_array(to_jsonb(receipt),to_jsonb(candidate),to_jsonb(activated),to_jsonb(submission))::text image FROM marketing_public_receipts receipt
 JOIN marketing_lead_candidates candidate ON candidate.id=receipt.candidate_id AND candidate.company_id=? AND candidate.form_id=?
 JOIN marketing_asset_events activated ON activated.asset_id=candidate.form_id AND activated.version=candidate.form_version AND activated.action='activate' AND activated.status='active'
 JOIN marketing_engagement_events submission ON submission.candidate_id=candidate.id AND submission.version=1 AND submission.event_type='form_submit'
 WHERE receipt.key=?`;

describe('marketing public-form image point-read dedupe', () => {
  it('emits byte-identical image point reads for all three variants', async () => {
    const { publicFormImageSql } = await import(
      '../../../packages/server-marketing/src/adapters/public-form-image-sql.ts'
    );
    expect(publicFormImageSql('form')).toBe(form);
    expect(publicFormImageSql('content')).toBe(content);
    expect(publicFormImageSql('receipt')).toBe(receipt);
  });

  it('keeps one query token in the helper and no inline literal in the consumer', async () => {
    const helper = await readFile(adapter('public-form-image-sql.ts'), 'utf8');
    const reader = await readFile(adapter('public-form-read.ts'), 'utf8');
    expect(helper.match(/SELECT/g)).toBeNull();
    expect(helper).toContain("marketingSelectHeadSql(projection, source)");
    const head = await readFile(adapter('select-head-sql.ts'), 'utf8');
    expect(head.match(/SELECT/g)).toHaveLength(1);
    expect(reader).not.toContain(form);
    expect(reader).not.toContain(content);
    expect(reader).not.toContain(receipt);
    expect(reader).toContain("publicFormImageSql('form')");
    expect(reader).toContain("publicFormImageSql('content')");
    expect(reader).toContain("publicFormImageSql('receipt')");
  });

  it('preserves the bind order of every image point read', async () => {
    const reader = await readFile(adapter('public-form-read.ts'), 'utf8');
    expect(reader).toContain("publicFormImageSql('form')).bind(id)");
    expect(reader).toContain("publicFormImageSql('content')).bind(refs.contentAssetId,form.company_id)");
    expect(reader).toContain(
      "publicFormImageSql('receipt')).bind(form.company_id,id,submissionKey)",
    );
  });
});
