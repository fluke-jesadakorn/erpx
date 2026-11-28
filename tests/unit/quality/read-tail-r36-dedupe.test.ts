import { describe, expect, test, vi } from 'vitest';
import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
vi.mock('server-only', () => ({}));
import { evidenceHead } from '../../../modules/quality/server/src/adapters/inspection-evidence-source-sql';
import { qualityAttachmentSourceSelect } from '../../../modules/quality/server/src/adapters/quality-attachment-source-select-sql';
import { qualityPurchaseSourceRows } from '../../../modules/quality/server/src/adapters/purchase-sources';

const srcRoot = resolve(__dirname, '../../../modules/quality/server/src');
const read = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');
const sha = (sql: string) => createHash('sha256').update(sql).digest('hex');

// Byte oracles captured at HEAD before the r36 fold, rendered by the retired
// inline `SELECT` in quality-attachment-source-select-sql. The helper now
// passes the same projection, FROM relation and two-bind WHERE tail to
// evidenceHead; the rendered SQL and bind order must not change.
const renderings = [
  {
    name: 'hold',
    branch: 'branch_id',
    kind: 'kind',
    image: 'witness::text image',
    from: `(${qualityPurchaseSourceRows}) actual_quality_source`,
    before: `SELECT id,company_id,branch_id,NULL::text department_id,kind,witness::text image FROM (${qualityPurchaseSourceRows}) actual_quality_source WHERE id=? AND company_id=?`,
    digest: 'f43a021c4b9060f771b29a1cadb1bbad90af03415271922c094f1d32a372937e',
  },
  {
    name: 'spec',
    branch: 'NULL::text branch_id',
    kind: "'advanced' kind",
    image: "(to_jsonb(s)-'version'-'updated_at')::text image",
    from: 'inventory_quality_specs s',
    before: "SELECT id,company_id,NULL::text branch_id,NULL::text department_id,'advanced' kind,(to_jsonb(s)-'version'-'updated_at')::text image FROM inventory_quality_specs s WHERE id=? AND company_id=?",
    digest: 'accb851cd449aaa9bfef339b2a1e8961473f1d46a849fa5f2e6818dc2f72b4e5',
  },
] as const;

const kpi = (pattern: RegExp) => {
  let total = 0;
  const walk = (dir: string) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = resolve(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (entry.name.endsWith('.ts')) {
        total += (readFileSync(path, 'utf8').match(pattern) ?? []).length;
      }
    }
  };
  walk(srcRoot);
  return total;
};

describe('quality read-tail r36 attachment source head dedupe', () => {
  test.each(renderings)('source $name is byte-identical to the pre-dedupe rendering', (r) => {
    const sql = qualityAttachmentSourceSelect(r.branch, r.kind, r.image, r.from);
    expect(sql).toBe(r.before);
    expect(sha(sql)).toBe(r.digest);
    expect(sql).toBe(evidenceHead(`id,company_id,${r.branch},NULL::text department_id,${r.kind},${r.image}`, ` FROM ${r.from}`, ' WHERE id=? AND company_id=?'));
    expect(sql.startsWith('SELECT id,company_id,')).toBe(true);
    expect(sql.endsWith(`${r.from} WHERE id=? AND company_id=?`)).toBe(true);
    expect(sql.match(/\?/g)).toHaveLength(2);
    expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('the helper delegates to evidenceHead and owns no inline SELECT', () => {
    const helper = read('adapters/quality-attachment-source-select-sql.ts');
    expect(helper).toContain('import { evidenceHead } from "./inspection-evidence-source-sql";');
    expect(helper).toContain("evidenceHead(`id,company_id,${branch},NULL::text department_id,${kind},${image}`, ` FROM ${from}`, ' WHERE id=? AND company_id=?')");
    expect(helper.match(/SELECT /g) ?? []).toHaveLength(0);
    const head = read('adapters/inspection-evidence-source-sql.ts');
    expect(head).toContain('export const evidenceHead = (projection: string, from: string, tail: string) => `SELECT ${projection}${from}${tail}`;');
    expect(head.match(/SELECT /g)).toHaveLength(1);
  });

  test('callers and the [targetId, companyId] bind order are unchanged', () => {
    const callers = read('adapters/attachment-purchases.ts');
    expect(callers).toContain('import {qualityAttachmentSourceSelect} from "./quality-attachment-source-select-sql";');
    expect(callers.match(/qualityAttachmentSourceSelect\(/g)).toHaveLength(2);
    expect(callers).toContain('? qualityAttachmentSourceSelect("branch_id","kind","witness::text image",`(${qualityPurchaseSourceRows}) actual_quality_source`)');
    expect(callers).toContain(': qualityAttachmentSourceSelect("NULL::text branch_id","\'advanced\' kind","(to_jsonb(s)-\'version\'-\'updated_at\')::text image","inventory_quality_specs s");');
    expect(callers).toContain('.bind(targetId,actor.companyId).first');
  });

  test('protected locks and native registry are untouched', () => {
    expect(read('adapters/purchase-sources.ts')).toContain('qualityPurchaseSourceReadLocks');
    expect(read('adapters/purchase-sources.ts')).toContain('LOCK TABLE inventory_quality_holds,inventory_quality_disposition_events,inventory_counts,inventory_count_lines,inventory_count_lot_lines IN SHARE MODE');
    expect(read('adapters/attachment-read-sources.ts')).toContain('LOCK TABLE inventory_quality_holds,inventory_quality_specs');
  });

  test('retirement:ts-statements is strictly lower than the BEFORE=26 baseline', () => {
    expect(kpi(/INSERT INTO|DELETE FROM|UPDATE [a-z_]+ SET|SELECT /g)).toBeLessThan(26);
    expect(kpi(/INSERT INTO|DELETE FROM|UPDATE [a-z_]+ SET|SELECT /g)).toBe(1);
    expect(kpi(/SELECT /g)).toBe(1);
  });
});
