import { describe, it, expect } from 'vitest';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import ts from 'typescript';
import { assignmentReadSql } from '@erpx/server-identity_access/public/record-source-authority';
import { marketingSourceRowGuardSql, attachmentOwnerVersionGuardSql } from '../../../packages/server-marketing/src/adapters/attachment-owner-version-guard';
import { marketingPointExistsSql } from '../../../packages/server-marketing/src/adapters/marketing-point-exists-sql';
import { marketingSelectHeadSql } from '../../../packages/server-marketing/src/adapters/select-head-sql';
import { marketingModuleMemberSql } from '../../../packages/server-marketing/src/adapters/module-membership';
import { publicFormImageSql } from '../../../packages/server-marketing/src/adapters/public-form-image-sql';
import { marketingRecordScopeAuthoritySql } from '../../../packages/server-marketing/src/adapters/record-scope-authority';

const adapter = (name: string) =>
  new URL(`../../../packages/server-marketing/src/adapters/${name}`, import.meta.url);
const src = (name: string) => readFile(adapter(name), 'utf8');

// Oracles frozen from base e313f4375f2100a8db74a4851395530db948f3ad (HEAD at
// task start; marketing paths unchanged since 46afec375). No other worker
// touched these paths before this packet.
const HEAD = 'e313f4375f2100a8db74a4851395530db948f3ad';

/** AST evidence: count SELECT tokens only inside string/template text nodes. */
function selectTokens(text: string): number {
  const file = ts.createSourceFile('owner.ts', text, ts.ScriptTarget.Latest, true);
  let total = 0;
  const visit = (node: ts.Node) => {
    if (
      ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node) ||
      ts.isTemplateHead(node) || ts.isTemplateMiddle(node) || ts.isTemplateTail(node)
    )
      total += (node.text.match(/SELECT /g) ?? []).length;
    ts.forEachChild(node, visit);
  };
  visit(file);
  return total;
}
async function ownerSelectTokens(): Promise<number> {
  let total = 0;
  const walk = async (dir: string) => {
    for (const entry of await readdir(dir, { withFileTypes: true })) {
      const path = resolve(dir, entry.name);
      if (entry.isDirectory()) await walk(path);
      else if (entry.name.endsWith('.ts')) total += selectTokens(await readFile(path, 'utf8'));
    }
  };
  await walk(resolve('packages/server-marketing/src'));
  return total;
}

// Byte oracles captured from the two source-row guard heads and the two
// SELECT-head renderers before the r46 fold.
const guard = (source: string, tail: string) =>
  `EXISTS(SELECT 1 FROM ${source} WHERE ${tail})`;
const point = (source: string, where: string) =>
  `SELECT 1 FROM ${source} WHERE ${where}`;
const member = () =>
  "SELECT 1 FROM assignments WHERE id=? AND (jsonb_array_length(modules_json::jsonb)=0 OR modules_json::jsonb @> jsonb_build_array('marketing'::text))";
const memberSelected = () =>
  "SELECT 1 FROM assignments selected WHERE selected.id=? AND (jsonb_array_length(selected.modules_json::jsonb)=0 OR selected.modules_json::jsonb @> jsonb_build_array('marketing'::text))";
const memberAllowed = (alias = '') =>
  `SELECT (jsonb_array_length(${alias ? `${alias}.` : ''}modules_json::jsonb)=0 OR ${alias ? `${alias}.` : ''}modules_json::jsonb @> jsonb_build_array('marketing'::text)) allowed FROM assignments WHERE id=?`;
const imageForm =
  "SELECT company_id,status,configuration_json,to_jsonb(form)::text image FROM marketing_assets form WHERE id=? AND kind='form'";
const imageContent =
  "SELECT to_jsonb(content)::text image FROM marketing_assets content WHERE id=? AND company_id=? AND kind='content'";
const imageReceipt =
  "SELECT jsonb_build_array(to_jsonb(receipt),to_jsonb(candidate),to_jsonb(activated),to_jsonb(submission))::text image FROM marketing_public_receipts receipt\n JOIN marketing_lead_candidates candidate ON candidate.id=receipt.candidate_id AND candidate.company_id=? AND candidate.form_id=?\n JOIN marketing_asset_events activated ON activated.asset_id=candidate.form_id AND activated.version=candidate.form_version AND activated.action='activate' AND activated.status='active'\n JOIN marketing_engagement_events submission ON submission.candidate_id=candidate.id AND submission.version=1 AND submission.event_type='form_submit'\n WHERE receipt.key=?";

describe('marketing read-tail r46 dedupe', () => {
  it(`folds the two point-existence heads byte-identically from ${HEAD}`, () => {
    for (const [source, tail] of [
      ['t', 'p'],
      ['(S) a', 'a.id=?'],
      ['marketing_assets', 'id=? AND company_id=? AND version=?'],
    ] as const) {
      expect(marketingPointExistsSql(source, tail)).toBe(point(source, tail));
      expect(marketingSourceRowGuardSql(source, tail)).toBe(guard(source, tail));
    }
    // No-gap admission and the six-placeholder public-submission witness.
    expect(marketingPointExistsSql('(SCOPE)workforce_actor_scope', '«where»', '')).toBe(
      'SELECT 1 FROM(SCOPE)workforce_actor_scope WHERE «where»',
    );
    expect(marketingPointExistsSql('marketing_public_receipts receipt JOIN (C) candidate ON candidate.id=receipt.candidate_id',
      'receipt.key=? AND candidate.company_id=?')).toBe(
      point('marketing_public_receipts receipt JOIN (C) candidate ON candidate.id=receipt.candidate_id',
        'receipt.key=? AND candidate.company_id=?'),
    );
    expect(marketingSourceRowGuardSql('t', 'p').match(/\?/g)).toBeNull();

    // The version-guard wrapper keeps every original bind and tail byte.
    expect(attachmentOwnerVersionGuardSql('marketing_assets')).toBe(
      guard('marketing_assets', 'id=? AND company_id=? AND version=?'),
    );
    expect(attachmentOwnerVersionGuardSql('marketing_assets', 'extra')).toBe(
      guard('marketing_assets', 'id=? AND company_id=? AND version=? AND extra'),
    );
    const rsa = marketingRecordScopeAuthoritySql('marketing_source', 'marketing_authority');
    expect(rsa).toBe(guard(
      `(SELECT marketing_source.company_id,COALESCE(marketing_source.branch_id,?::text) branch_id,COALESCE(marketing_source.department_id,?::text) department_id) marketing_authority`,
      assignmentReadSql('marketing_authority', 'records.read'),
    ));
  });

  it(`folds the two SELECT heads byte-identically from ${HEAD}`, () => {
    expect(marketingSelectHeadSql('t')).toBe('SELECT t');
    expect(marketingSelectHeadSql('t', ' FROM s WHERE id=?')).toBe('SELECT t FROM s WHERE id=?');

    expect(marketingModuleMemberSql()).toBe(member());
    expect(marketingModuleMemberSql('selected')).toBe(memberSelected());
    expect(marketingModuleMemberSql('', 'allowed')).toBe(memberAllowed());
    expect(marketingModuleMemberSql('m', 'allowed')).toBe(memberAllowed('m'));

    expect(publicFormImageSql('form')).toBe(imageForm);
    expect(publicFormImageSql('content')).toBe(imageContent);
    expect(publicFormImageSql('receipt')).toBe(imageReceipt);
    expect((publicFormImageSql('receipt').match(/\?/g) ?? [])).toHaveLength(3);
    expect(imageForm).toContain("kind='form'");
    expect(imageContent).toContain("kind='content'");
    expect(imageReceipt).toContain('WHERE receipt.key=?');
  });

  it('keeps one terminal token in each renderer and none in the four wrappers', async () => {
    const pointHead = await src('marketing-point-exists-sql.ts');
    expect(pointHead).toContain('export const marketingPointExistsSql');
    expect(pointHead).toContain('`SELECT 1 FROM${gap}${source} WHERE ${where}`');
    expect(selectTokens(pointHead)).toBe(1);

    const selectHead = await src('select-head-sql.ts');
    expect(selectHead).toContain('export const marketingSelectHeadSql');
    expect(selectHead).toContain('`SELECT ${head}${tail}`');
    expect(selectHead.match(/SELECT \$\{/g)).toHaveLength(1);
    expect(selectTokens(selectHead)).toBe(1);

    const guardBody = await src('attachment-owner-version-guard.ts');
    expect(guardBody).not.toContain('EXISTS(SELECT 1 FROM ${');
    expect(guardBody).toContain('`EXISTS(${marketingPointExistsSql(source, tail)})`');
    expect(selectTokens(guardBody)).toBe(0);

    const memberBody = await src('module-membership.ts');
    expect(memberBody).toContain('marketingSelectHeadSql(head, tail)');
    expect(selectTokens(memberBody)).toBe(0);

    const imageBody = await src('public-form-image-sql.ts');
    expect(imageBody).toContain('marketingSelectHeadSql(projection, source)');
    expect(selectTokens(imageBody)).toBe(0);
  });

  it('leaves the protected r38/r40/r45 heads, native reads and settlement untouched', async () => {
    // Protected helper bodies: exactly one terminal token, unchanged text.
    expect((await src('form-asset-read-head.ts')).match(/SELECT /g)).toHaveLength(1);
    expect((await src('owner-point-read-sql.ts')).match(/SELECT /g)).toHaveLength(1);
    expect((await src('owner-projection-source.ts')).match(/SELECT /g)).toHaveLength(1);
    expect(await src('read-tail-sql.ts')).toContain('`SELECT * FROM ${table}${tail}`');
    expect((await src('read-tail-sql.ts')).match(/SELECT/g)).toHaveLength(1);

    // Native read kinds and settlement receipts keep their own statements.
    expect(await src('record-purchases.ts')).toContain("marketingPointExistsSql(`(${nativeIds}) actual`,'id=?')");
    expect(await src('record-purchases.ts')).not.toContain('`SELECT 1 FROM (${nativeIds}) actual WHERE id=?`');
    expect(await src('purchases.ts')).toContain('nativeMarketingReadFirst<Record<string,unknown>>(MARKETING_RD_COMMAND_RECEIPT_PRIOR');
    expect(await src('package-obligations.ts')).toContain('marketingPointExistsSql(');
    expect(await src('native-marketing-read.ts')).toContain('export');

    // FOR SHARE locks and ORDER/FOR UPDATE tails stay byte-identical.
    expect(await src('form-asset-lock-sql.ts')).toContain(
      '`SELECT id FROM marketing_assets WHERE id=? AND company_id=?${scope} FOR SHARE`',
    );
    expect(await src('marketing-company-list-sql.ts')).toContain('ORDER BY updated_at DESC');
    expect(await src('marketing-local-point-read-sql.ts')).toContain('FOR UPDATE');
  });

  it('pins the owner SELECT KPI to 12 (lane 40 retired 3) (BEFORE=34)', async () => {
    expect(await ownerSelectTokens()).toBe(12);
    expect(await ownerSelectTokens()).toBeLessThan(32);
  });
});
