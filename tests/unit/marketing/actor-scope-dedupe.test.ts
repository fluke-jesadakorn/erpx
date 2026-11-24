import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';

const adapter = (name: string) =>
  new URL(`../../../packages/server-marketing/src/adapters/${name}`, import.meta.url);

const scope = '(SELECT ?::text company_id,?::text branch_id,?::text department_id)';

describe('marketing actor-scope subquery dedupe', () => {
  it('keeps one canonical scope literal and no inline copies in consumers', async () => {
    const canonical = await readFile(adapter('marketing-actor-scope-sql.ts'), 'utf8');
    expect(canonical.match(/SELECT/g)).toHaveLength(1);

    for (const name of [
      'purchases.ts',
      'attachment-purchases.ts',
      'metric-purchases.ts',
      'attachment-reviewer-guard.ts',
      'attachment-read.ts',
    ]) {
      const source = await readFile(adapter(name), 'utf8');
      expect(source).not.toContain(scope);
      expect(source).toContain('marketingActorScopeSql');
    }
  });

  it('emits the original byte-identical scope for the canonical const', async () => {
    const { marketingActorScopeSql } = await import(
      '../../../packages/server-marketing/src/adapters/marketing-actor-scope-sql.ts'
    );
    expect(marketingActorScopeSql).toBe(scope);
  });

  it('preserves the original emitted reviewer-guard suffix and alias spacing', async () => {
    const { attachmentReviewerGuardSql } = await import(
      '../../../packages/server-marketing/src/adapters/attachment-reviewer-guard.ts'
    );
    expect(attachmentReviewerGuardSql()).toContain(` FROM ${scope} marketing_file_review`);
    expect(attachmentReviewerGuardSql('allowed')).toContain(` allowed FROM ${scope} marketing_file_review`);
  });

  it('preserves every bind site next to the shared scope', async () => {
    const purchases = await readFile(adapter('purchases.ts'), 'utf8');
    const attachments = await readFile(adapter('attachment-purchases.ts'), 'utf8');
    const metric = await readFile(adapter('metric-purchases.ts'), 'utf8');
    const read = await readFile(adapter('attachment-read.ts'), 'utf8');
    expect(purchases).toContain('allowed FROM ${marketingActorScopeSql} ${alias}');
    expect(attachments).toContain('FROM ${marketingActorScopeSql} marketing_file_scope)');
    expect(metric).toContain('allowed FROM ${marketingActorScopeSql} marketing_metric_scope');
    expect(read).toContain('FROM${marketingActorScopeSql}workforce_actor_scope');
    expect(purchases).toContain('.bind(actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId)');
    expect(attachments).toContain('[actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId]');
    expect(metric).toContain('.bind(actor.assignmentId,actor.userId,actor.role,actor.assignmentId,actor.companyId,actor.branchId,actor.departmentId)');
    expect(read).toContain('.bind(actor.companyId,actor.branchId,actor.departmentId,...current.args)');
  });
});
