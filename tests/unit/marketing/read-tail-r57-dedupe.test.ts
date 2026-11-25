import { describe, it, expect } from 'vitest';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import ts from 'typescript';

const adapter = (name: string) =>
  new URL(`../../../packages/server-marketing/src/adapters/${name}`, import.meta.url);
const src = (name: string) => readFile(adapter(name), 'utf8');

/** AST evidence: count SELECT tokens only inside string/template text nodes. */
function headTokens(text: string): number {
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
async function ownerTokens(): Promise<{ spaced: number; bare: number }> {
  let spaced = 0,
    bare = 0;
  const walk = async (dir: string) => {
    for (const entry of await readdir(dir, { withFileTypes: true })) {
      const path = resolve(dir, entry.name);
      if (entry.isDirectory()) await walk(path);
      else if (entry.name.endsWith('.ts')) {
        const text = await readFile(path, 'utf8');
        spaced += headTokens(text);
        bare += (text.match(/SELECT/g) ?? []).length;
      }
    }
  };
  await walk(resolve('packages/server-marketing/src'));
  return { spaced, bare };
}

describe('marketing read-tail r57 command-receipt guard cutover', () => {
  it('routes the purchase prior-receipt guard through the native owner plan', async () => {
    const purchases = await src('purchases.ts');
    expect(purchases.match(/SELECT /g)).toBeNull();
    expect(purchases).not.toContain("SELECT 1 FROM command_receipts WHERE command_id=?");
    expect(purchases).toContain('nativeMarketingReadFirst<Record<string,unknown>>(MARKETING_RD_COMMAND_RECEIPT_PRIOR');
    expect(purchases).toContain('{commandId:command.key,operation:command.operation,recordId:command.target.id,actorId:actor.userId,companyId:actor.companyId},database)');
  });

  it('registers the closed kind and its five ordered scope fields on the transport', async () => {
    const transport = await src('native-marketing-read.ts');
    expect(transport).toContain("export const MARKETING_RD_COMMAND_RECEIPT_PRIOR = 'command-receipt-prior';");
    expect(transport).toContain('MARKETING_RD_COMMAND_RECEIPT_PRIOR,\n]);');
    for (const field of ['commandId?: string', 'operation?: string', 'recordId?: string', 'actorId?: string'])
      expect(transport).toContain(field);
    expect(transport).toContain('scope.commandId, scope.operation, scope.recordId, scope.actorId');
  });

  it('leaves the shared point-existence head and purchase locks intact', async () => {
    expect((await src('marketing-point-exists-sql.ts')).match(/SELECT /g)).toHaveLength(1);
    expect(await src('purchases.ts')).toContain('LOCK TABLE marketing_campaign_costs,marketing_public_limits,marketing_public_receipts IN SHARE ROW EXCLUSIVE MODE');
    expect(await src('public-form.ts')).toContain('FOR SHARE');
  });

  it('retirement:ts-statements is BEFORE=13 -> AFTER=12', async () => {
    expect(await ownerTokens()).toEqual({ spaced: 8, bare: 8 });
  });
});
