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

describe('marketing read-tail r58 form lock cutover', () => {
  it('deletes the FOR SHARE head and plans both lock reads on the owner port', async () => {
    await expect(src('form-asset-lock-sql.ts')).rejects.toThrow();
    const form = await src('public-form.ts');
    const submission = await src('public-submission.ts');
    expect(form).toContain('nativeMarketingPlan(MARKETING_RD_FORM_LOCK)');
    expect(submission).toContain('nativeMarketingPlan(MARKETING_RD_FORM_LOCK_VERSIONED)');
    expect(form).toContain('`${lock.sql} FOR SHARE`');
    expect(submission).toContain('`${lock.sql} FOR SHARE`');
    expect(form.match(/SELECT /g)).toBeNull();
    expect(submission.match(/FROM marketing_assets WHERE id=\?/g)).toBeNull();
  });

  it('keeps the prepared-presentation port awaitable through the caller chain', async () => {
    const app = await readFile(new URL('../../../packages/server-marketing/src/application/marketing.ts', import.meta.url), 'utf8');
    expect(app).toContain('preparePresentation(key:string,configuration:unknown):Promise<Prepared>');
    expect(app).toContain('await repository.preparePresentation(key,source.configuration_json)');
    expect(app).toContain('await repository.preparePresentation(key,row.configuration_json)');
    const repo = await src('marketing.ts');
    expect(repo).toContain('preparePresentation:async (key,configuration)=>marketingPresentationAssertions');
  });

  it('leaves the shared point-existence and receipt heads intact', async () => {
    expect((await src('marketing-point-exists-sql.ts')).match(/SELECT /g) ?? []).toHaveLength(0);
    expect((await src('owner-point-read-sql.ts')).match(/SELECT /g) ?? []).toHaveLength(0);
  });

  it('retirement:ts-statements stays strictly below BEFORE=12 at 10 after r59', async () => {
    expect(await ownerTokens()).toEqual({ spaced: 3, bare: 3 });
  });
});
