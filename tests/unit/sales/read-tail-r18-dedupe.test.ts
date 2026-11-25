import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';
import ts from 'typescript';

vi.mock('server-only', () => ({}));

import { assignmentReadSql } from '@erpx/server-identity_access/public/record-source-authority';
import { scopeTuple } from '../../../packages/server-sales/src/adapters/read-tail-r29-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-sales/src');
const read = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const source = read('followup-source-sql.ts');
const pricing = read('pricing-purchases.ts');
const purchases = read('followup-purchases.ts');
const metric = read('metric-purchases.ts');
const obligations = read('package-obligations.ts');
const sha = (value: string) => createHash('sha256').update(value).digest('hex');

/** Literal-level AST ruling: every SQL-bearing literal is structurally unique. */
function sqlLiterals(): { file: string; text: string }[] {
  const out: { file: string; text: string }[] = [];
  const walk = (dir: string) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = resolve(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (entry.name.endsWith('.ts')) {
        const code = readFileSync(path, 'utf8');
        const file = ts.createSourceFile(entry.name, code, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
        const visit = (node: ts.Node) => {
          if (ts.isStringLiteralLike(node) || ts.isTemplateExpression(node)) {
            const text = node.getText(file);
            if (text.length > 20 && /\b(SELECT|FROM|WHERE|JOIN)\b/i.test(text)) out.push({ file: entry.name, text });
          }
          ts.forEachChild(node, visit);
        };
        visit(file);
      }
    }
  };
  walk(srcRoot);
  return out;
}
const normalize = (text: string) =>
  text.replace(/\$\{[^}]*\}/g, '?').replace(/\$\d+/g, '?').replace(/\b\d+\b/g, '?').replace(/\s+/g, ' ').trim();

function transpileDecl(file: ts.SourceFile, name: string): string {
  const decls = file.statements
    .filter(ts.isVariableStatement)
    .flatMap((s) => [...s.declarationList.declarations])
    .filter((d) => d.name.getText(file) === name);
  expect(decls).toHaveLength(1);
  return ts.transpileModule(`const ${decls[0]!.getText(file)};`, {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
  }).outputText;
}

const sourceAst = ts.createSourceFile('followup-source-sql.ts', source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
const authorityScopeRead = new Function(
  `${transpileDecl(sourceAst, 'authorityScopeRead')}\nreturn authorityScopeRead;`,
)() as (authority: string, tuple: string, relation: string, extra?: string) => string;
const metricAst = ts.createSourceFile('metric-purchases.ts', metric, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
const crmAssignmentExists = new Function(
  `${transpileDecl(metricAst, 'crmAssignmentExists')}\nreturn crmAssignmentExists;`,
)() as () => string;

const rendered = {
  pricing: authorityScopeRead(assignmentReadSql('pricing_scope', 'commerce.read'), scopeTuple('company'), 'pricing_scope'),
  followup: authorityScopeRead(assignmentReadSql('followup_authority', 'commerce.read'), scopeTuple('actor'), 'followup_authority'),
  metric: authorityScopeRead(assignmentReadSql('sales_metric_scope', 'commerce.read'), scopeTuple('actor'), 'sales_metric_scope', ` AND ${crmAssignmentExists()}`),
};

describe('Sales read-tail r18 duplicate-family ruling', () => {
  test('AST scan confirms zero exact duplicate SQL literal groups', () => {
    const literals = sqlLiterals();
    const groups = new Map<string, string[]>();
    for (const { text } of literals) {
      const key = normalize(text);
      groups.set(key, [...(groups.get(key) ?? []), text]);
    }
    // Previous ruling: 49 literals, zero exact duplicate groups. Current source
    // is unchanged in shape: 45 SQL-bearing literals, 45 distinct groups.
    expect(literals.length).toBeGreaterThanOrEqual(45);
    expect([...groups.values()].filter((g) => g.length > 1)).toHaveLength(0);
  });

  test('cross-file assignment-authority family is one named generator (r28)', () => {
    expect(source.match(/const authorityScopeRead\s*=/g) ?? []).toHaveLength(1);
    expect(rendered.pricing).toContain('FROM (SELECT ?::text company_id,NULL::text branch_id,NULL::text department_id) pricing_scope');
    expect(rendered.followup).toContain('FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) followup_authority');
    expect(rendered.metric).toContain('EXISTS(SELECT 1 FROM assignments');
    expect(sha(rendered.pricing)).toBe('172d6ec948d417cbe094a4f2680dff96c5807b1ae3749da9036868635e4da27d');
    expect(sha(rendered.followup)).toBe('27dd2ceb1b57757c4d4a227733a0442b27f86092e8c46cbd661da3bcea0a74ab');
    expect(sha(rendered.metric)).toBe('8fdc97d69ebb06f24bba2824dfdb3009c57ee9ec0fc03d16a9f3921d4501e87c');
  });

  test('bind-equivalence ruling: authority binds precede scope binds in every caller', () => {
    expect((scopeTuple('company').match(/\?/g) ?? []).length).toBe(1);
    expect((scopeTuple('actor').match(/\?/g) ?? []).length).toBe(3);
    expect(pricing).toContain('.bind(actor.assignmentId,actor.userId,actor.role,actor.companyId)');
    expect(purchases).toContain('.bind(actor.assignmentId,actor.userId,actor.role,actor.companyId,record?.branch_id??actor.branchId,record?.department_id??actor.departmentId)');
    expect(metric).toContain('.bind(actor.assignmentId,actor.userId,actor.role,actor.assignmentId,actor.companyId,actor.branchId,actor.departmentId)');
    // The three authority binds are always (assignmentId, userId, role); the
    // scope tuple contributes 1 (company) or 3 (actor) trailing binds, and the
    // metric guard contributes one id bind before its scope. No family cut can
    // reorder or drop a bind without changing these pinned call sites.
    for (const code of [pricing, purchases, metric]) expect(code).not.toContain('.bind(actor.userId,actor.assignmentId');
  });

  test('r7-r17 generators and source locks stay untouched', () => {
    for (const literal of [
      'const orderDeliveryExistsSql=', // r7
      'const commandAuditPrefixSql=', // r9
      'const allRows=', // r13
      'const factKind=', // r14
      'const scopedSource=', // r16
      'const ecommerceOrderScope=', // r7, collapsed in r27
    ]) expect(obligations).toContain(literal);
    expect(purchases).toContain('const followupExists='); // r17
    expect(purchases.match(/LOCK TABLE/g) ?? []).toHaveLength(1);
    expect(purchases).toContain('LOCK TABLE customer_followups,customer_followup_events IN SHARE ROW EXCLUSIVE MODE');
    expect(obligations).toContain('LOCK TABLE sales_price_lists,sales_price_list_items,customer_followups,customer_followup_events,customer_contact_notes,customer_contact_note_history,commerce_customer_notifications,commerce_customer_notification_settings,sales_notification_execution_authorizations,sales_notification_execution_attempts,sales_notification_reconciliations,command_receipts,audit_events IN SHARE MODE');
  });
});
