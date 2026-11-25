import { describe, it, expect } from 'vitest';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import ts from 'typescript';
import {
  marketingEvidenceCandidateRows,
  marketingEvidenceEventRows,
} from '../../../packages/server-marketing/src/adapters/executive-evidence-rows';

const adapter = (name: string) =>
  new URL(`../../../packages/server-marketing/src/adapters/${name}`, import.meta.url);
const src = (name: string) => readFile(adapter(name), 'utf8');
const srcRoot = resolve('packages/server-marketing/src');

// Byte oracles captured from the two original emitters before the r42 tail merge.
const candidateProjection =
  'c.id,c.contact_name,c.status,c.form_version,c.version,c.created_at,c.updated_at,c.branch_id,c.department_id,f.name form_name';
const eventProjection =
  'e.id,e.candidate_id,e.event_type,e.channel,e.version event_version,e.created_at,c.contact_name,c.status candidate_status,c.version candidate_version,c.branch_id,c.department_id,f.name form_name';
const assetJoin = ' JOIN marketing_assets f ON f.id=c.form_id AND f.company_id=c.company_id';
const candidate = (source: string) => `SELECT ${candidateProjection}\n      FROM ${source} c${assetJoin}`;
const event = (source: string) =>
  `SELECT ${eventProjection}\n      FROM marketing_engagement_events e JOIN ${source} c ON c.id=e.candidate_id${assetJoin}`;

/** AST evidence: count SELECT tokens only in string/template text nodes, never
 * comments or identifiers. */
function selectTokens(text: string): number {
  const file = ts.createSourceFile('owner.ts', text, ts.ScriptTarget.Latest, true);
  let total = 0;
  const visit = (node: ts.Node) => {
    if (
      ts.isStringLiteral(node) ||
      ts.isNoSubstitutionTemplateLiteral(node) ||
      ts.isTemplateHead(node) ||
      ts.isTemplateMiddle(node) ||
      ts.isTemplateTail(node)
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
  await walk(srcRoot);
  return total;
}

describe('marketing evidence-row tail merge (r42)', () => {
  it('renders both evidence row tails byte-identically from the closed variant builder', () => {
    for (const source of ['marketing_lead_candidates', '(SRC)']) {
      expect(marketingEvidenceCandidateRows(source)).toBe(candidate(source));
      expect(marketingEvidenceEventRows(source)).toBe(event(source));
    }
    expect(candidate('s').match(/ JOIN marketing_assets f ON f.id=c.form_id AND f.company_id=c.company_id/g)).toHaveLength(1);
    expect(event('s').match(/ JOIN marketing_assets f ON f.id=c.form_id AND f.company_id=c.company_id/g)).toHaveLength(1);
    expect(marketingEvidenceCandidateRows('s')).not.toContain('marketing_engagement_events');
    expect(marketingEvidenceEventRows('s')).toContain('marketing_engagement_events e JOIN s c ON c.id=e.candidate_id');
  });

  it('keeps one SELECT token and one asset join in the head AST with two delegating exports', async () => {
    const head = await src('executive-evidence-rows.ts');
    expect(selectTokens(head)).toBe(1);
    expect(head.match(/ JOIN marketing_assets f ON f.id=c.form_id AND f.company_id=c.company_id/g)).toHaveLength(1);

    const file = ts.createSourceFile('executive-evidence-rows.ts', head, ts.ScriptTarget.Latest, true);
    const variant = file.statements.find(
      (node): node is ts.TypeAliasDeclaration =>
        ts.isTypeAliasDeclaration(node) && node.name.text === 'EvidenceVariant',
    );
    expect(variant).toBeDefined();
    expect((variant!.type as ts.UnionTypeNode).types.map((node) => node.getText(file).replace(/'/g, '')).sort()).toEqual([
      'candidate',
      'event',
    ]);

    const builders = file.statements.filter(
      (node): node is ts.FunctionDeclaration =>
        ts.isFunctionDeclaration(node) && node.name?.text === 'evidenceRows',
    );
    expect(builders).toHaveLength(1);
    expect(selectTokens(builders[0]!.getText(file))).toBe(1);

    const exports = file.statements.filter(
      (node): node is ts.FunctionDeclaration =>
        ts.isFunctionDeclaration(node) &&
        !!node.name &&
        !!node.modifiers?.some((modifier) => modifier.kind === ts.SyntaxKind.ExportKeyword),
    );
    expect(exports.map((node) => node.name!.text)).toEqual([
      'marketingEvidenceCandidateRows',
      'marketingEvidenceEventRows',
    ]);
    const delegated: string[] = [];
    for (const fn of exports) {
      expect(selectTokens(fn.getText(file))).toBe(0);
      const visit = (node: ts.Node) => {
        if (ts.isCallExpression(node) && node.expression.getText(file) === 'evidenceRows')
          delegated.push(node.arguments.map((argument) => argument.getText(file)).join(','));
        ts.forEachChild(node, visit);
      };
      visit(fn);
    }
    expect(delegated).toEqual(["'candidate',candidateSource", "'event',candidateSource"]);
  });

  it('keeps the two source consumers and their binds unchanged', async () => {
    const evidence = await src('executive-evidence-sources.ts');
    expect(evidence).toContain(
      'import {marketingEvidenceCandidateRows,marketingEvidenceEventRows} from "./executive-evidence-rows"',
    );
    expect(evidence.match(/marketingEvidenceCandidateRows\(candidateSource\)/g)).toHaveLength(1);
    expect(evidence.match(/marketingEvidenceEventRows\(candidateSource\)/g)).toHaveLength(1);
    expect(evidence).toContain('ORDER BY c.contact_name,c.id LIMIT ?');
    expect(evidence).toContain('ORDER BY e.created_at DESC,e.id DESC LIMIT ?');

    const marketing = await src('marketing.ts');
    expect(marketing).toContain(
      'import {marketingEvidenceCandidateRows,marketingEvidenceEventRows} from "./executive-evidence-rows"',
    );
    expect(marketing.match(/marketingEvidenceCandidateRows\('marketing_lead_candidates'\)/g)).toHaveLength(1);
    expect(marketing.match(/marketingEvidenceEventRows\('marketing_lead_candidates'\)/g)).toHaveLength(1);
    expect(marketing).toContain(
      '.bind(recordId,a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId).first<Row>();if(!range)return null;',
    );
    expect(marketing).toContain('range.from,range.toExclusive).first<Row>();');
  });

  it('leaves FOR SHARE, settlement receipts and native read ports untouched', async () => {
    expect(await src('form-asset-lock-sql.ts')).toContain(
      '`SELECT id FROM marketing_assets WHERE id=? AND company_id=?${scope} FOR SHARE`',
    );
    expect(await src('purchases.ts')).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(await src('native-marketing-read.ts')).toContain('export');
    expect(await src('marketing.ts')).toContain('from "./native-marketing-read"');
  });

  it('confirms the owner KPI of 34 from the AST after the 36 -> 34 r45 drop', async () => {
    expect(await ownerSelectTokens()).toBe(34);
    expect(await ownerSelectTokens()).toBeLessThan(36);
  });
});
