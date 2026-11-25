import { describe, it, expect } from 'vitest';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import ts from 'typescript';
import { assignmentReadSql } from '@erpx/server-identity_access/public/record-source-authority';
import { marketingRecordScopeAuthoritySql } from '../../../packages/server-marketing/src/adapters/record-scope-authority';
import { marketingSourceRowGuardSql } from '../../../packages/server-marketing/src/adapters/attachment-owner-version-guard';
import { marketingPointExistsSql } from '../../../packages/server-marketing/src/adapters/marketing-point-exists-sql';
import { publicSubmissionReceiptWitness } from '../../../packages/server-marketing/src/adapters/crm-purchase-sources';

const adapter = (name: string) =>
  new URL(`../../../packages/server-marketing/src/adapters/${name}`, import.meta.url);
const src = (name: string) => readFile(adapter(name), 'utf8');

// Oracles are frozen from base 0146e08219e796645b1706246be7c86eeacc42d6 (HEAD
// at task start); no other worker touched these files before this packet.
const HEAD = '0146e08219e796645b1706246be7c86eeacc42d6';

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

const scopeTuple = (source: string, authority: string) =>
  `(SELECT ${source}.company_id,COALESCE(${source}.branch_id,?::text) branch_id,COALESCE(${source}.department_id,?::text) department_id) ${authority}`;
const rsaOracle = (source: string, authority: string) =>
  `EXISTS(SELECT 1 FROM ${scopeTuple(source, authority)} WHERE ${assignmentReadSql(authority, 'records.read')})`;

const candidateSourcesOracle = `SELECT candidate.* FROM marketing_lead_candidates candidate
 JOIN marketing_assets form ON form.id=candidate.form_id AND form.company_id=candidate.company_id AND form.kind='form' AND form.version>=candidate.form_version
 JOIN marketing_asset_events captured ON captured.asset_id=form.id AND captured.version=candidate.form_version AND captured.action='activate' AND captured.status='active'
 JOIN marketing_engagement_events submission ON submission.candidate_id=candidate.id AND submission.version=1 AND submission.event_type='form_submit'
 WHERE length(trim(candidate.consent_evidence))>=3`;
const witnessOracle = 'SELECT 1 FROM marketing_public_receipts receipt JOIN (' +
  candidateSourcesOracle + ') candidate ON candidate.id=receipt.candidate_id WHERE receipt.key=? AND candidate.company_id=? AND candidate.form_id=? AND candidate.form_version=?';

describe('marketing read-tail r44 dedupe', () => {
  it(`renders the retired record-scope authority head byte-identically from ${HEAD}`, async () => {
    const helper = await src('record-scope-authority.ts');
    expect(helper).toContain('export const marketingRecordScopeAuthoritySql');
    expect(helper).toContain('marketingSourceRowGuardSql(');
    expect(helper.match(/SELECT/g)).toHaveLength(1);

    for (const [source, authority] of [
      ['marketing_source', 'marketing_authority'],
      ['marketing_report', 'marketing_scope'],
      ['src', 'auth'],
    ] as const) {
      expect(marketingRecordScopeAuthoritySql(source, authority)).toBe(rsaOracle(source, authority));
    }
    expect(marketingSourceRowGuardSql('t', 'p')).toBe('EXISTS(SELECT 1 FROM t WHERE p)');
  });

  it('renders both retired point-existence heads byte-identically from HEAD', async () => {
    // Public-submission witness: the shared head replaces the inline literal.
    const sources = await src('crm-purchase-sources.ts');
    expect(sources).not.toContain(witnessOracle);
    expect(sources).toContain('export const publicSubmissionReceiptWitness=marketingPointExistsSql(');
    expect(publicSubmissionReceiptWitness).toBe(witnessOracle);

    // Workforce-actor-scope probe: the no-gap admission preserves FROM bytes.
    const attachment = await src('attachment-read.ts');
    expect(attachment).not.toContain('`SELECT 1 FROM${marketingActorScopeSql}workforce_actor_scope WHERE ${current.where}`');
    expect(attachment).toContain("marketingPointExistsSql(`${marketingActorScopeSql}workforce_actor_scope`,current.where,'')");
    expect(marketingPointExistsSql('(SCOPE)workforce_actor_scope', '«where»', '')).toBe(
      'SELECT 1 FROM(SCOPE)workforce_actor_scope WHERE «where»',
    );
  });

  it('keeps one point-existence head token and one source-row guard head token', async () => {
    expect(selectTokens(await src('marketing-point-exists-sql.ts'))).toBe(1);
    expect(selectTokens(await src('attachment-owner-version-guard.ts'))).toBe(0);
    expect(selectTokens(await src('record-scope-authority.ts'))).toBe(1);
  });

  it('leaves the cross-owner-pinned guard, FOR SHARE, command_receipts settlement and native ports untouched', async () => {
    expect(await src('record-purchases.ts')).toContain('`SELECT 1 FROM (${nativeIds}) actual WHERE id=?`');
    expect(await src('form-asset-lock-sql.ts')).toContain('FOR SHARE');
    expect(await src('purchases.ts')).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(await src('package-obligations.ts')).toContain('EXISTS(SELECT 1 FROM marketing_assets form');
    expect(await src('native-marketing-read.ts')).toContain('export');
  });

  it('pins the owner SELECT KPI to 32 (BEFORE=36)', async () => {
    expect(await ownerSelectTokens()).toBe(32);
    expect(await ownerSelectTokens()).toBeLessThan(36);
  });
});
