import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
import { qualityLineageTerminalSql } from '../../../packages/server-quality/src/adapters/quality-lineage-terminal-sql';
import { inspectionEvidenceSource, inspectionMetricSource } from '../../../packages/server-quality/src/adapters/inspection-evidence-source-sql';
import { commercialOwnerProjectionSources as commerce } from '@erpx/server-commerce/public/commercial-owner-projection-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-quality/src');
const read = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');

// Pre-dedupe lineage terminal heads copied byte-for-byte from the r23 source.
const terminals = [
  {
    name: 'purchase remaining-hold root',
    columns: '*',
    tail: 'ORDER BY depth DESC LIMIT 1',
    before: 'SELECT * FROM lineage ORDER BY depth DESC LIMIT 1',
  },
  {
    name: 'purchase roots CTE',
    columns: 'DISTINCT ON(hold_id) hold_id,root_id',
    tail: 'ORDER BY hold_id,depth DESC',
    before: 'SELECT DISTINCT ON(hold_id) hold_id,root_id FROM lineage ORDER BY hold_id,depth DESC',
  },
  {
    name: 'sales settlement terminal',
    columns: 'DISTINCT ON(h.id,lineage.source_company) h.*,lineage.source_company,lineage.depth source_depth',
    tail: 'JOIN inventory_quality_holds h ON h.id=lineage.id ORDER BY h.id,lineage.source_company,lineage.depth DESC',
    before: 'SELECT DISTINCT ON(h.id,lineage.source_company) h.*,lineage.source_company,lineage.depth source_depth FROM lineage JOIN inventory_quality_holds h ON h.id=lineage.id ORDER BY h.id,lineage.source_company,lineage.depth DESC',
  },
  {
    name: 'ecommerce settlement terminal',
    columns: 'DISTINCT id',
    tail: '',
    before: 'SELECT DISTINCT id FROM lineage',
  },
] as const;

const evidenceJoin = `\n    FROM inventory_quality_inspections i
    JOIN inventory_quality_holds h ON h.id=i.hold_id
    JOIN (${commerce.commerce_products}) p ON p.id=h.product_id AND p.company_id=h.company_id
    JOIN (${commerce.commerce_warehouses}) w ON w.id=h.warehouse_id AND w.company_id=h.company_id`;
const evidenceList = 'i.id,i.hold_id,i.result,i.created_at,i.reviewed_version,p.name product_name,w.name warehouse_name';
const evidenceDetail = 'i.id,i.hold_id,i.result,i.evidence,i.inspected_by,i.created_at,i.reviewed_version,\n      p.name product_name,w.name warehouse_name';
const metricLabel = "'Company-wide'";
const metricProjection = `${metricLabel} label,\n        count(*) FILTER (WHERE i.result='pass')::text numerator,count(*)::text denominator`;
const metricTail = '\n      WHERE h.company_id=? AND (?::text IS NULL OR h.branch_id=?) AND i.created_at>=? AND i.created_at<?\n      GROUP BY 1 ORDER BY 1';
const metricBefore = `SELECT ${metricLabel} label,
        count(*) FILTER (WHERE i.result='pass')::text numerator,count(*)::text denominator
      FROM inventory_quality_inspections i JOIN inventory_quality_holds h ON h.id=i.hold_id
      WHERE h.company_id=? AND (?::text IS NULL OR h.branch_id=?) AND i.created_at>=? AND i.created_at<?
      GROUP BY 1 ORDER BY 1`;

describe('quality lineage terminal head dedupe (r24)', () => {
  test.each(terminals)('$name: helper reproduces the pre-dedupe terminal byte-for-byte', ({ columns, tail, before }) => {
    const sql = qualityLineageTerminalSql(columns, tail);
    expect(sql).toBe(before);
    expect(sql.startsWith('SELECT ')).toBe(true);
    expect(sql.match(/SELECT /g)).toHaveLength(1);
    expect(sql).not.toContain('?');
    expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });
  test('the four readers share one helper with no residual inline terminals', () => {
    const purchase = read('adapters/purchase-sources.ts');
    const sales = read('adapters/sales-package-sources.ts');
    const ecommerce = read('adapters/ecommerce-package-sources.ts');
    expect(purchase.match(/qualityLineageTerminalSql\(/g)).toHaveLength(2);
    expect(sales.match(/qualityLineageTerminalSql\(/g)).toHaveLength(1);
    expect(ecommerce.match(/qualityLineageTerminalSql\(/g)).toHaveLength(1);
    for (const { before } of terminals) {
      expect(purchase).not.toContain(before);
      expect(sales).not.toContain(before);
      expect(ecommerce).not.toContain(before);
    }
    expect(purchase).toContain('import {qualityLineageTerminalSql} from "./quality-lineage-terminal-sql"');
    expect(sales).toContain("import {qualityLineageTerminalSql} from './quality-lineage-terminal-sql'");
    expect(ecommerce).toContain("import {qualityLineageTerminalSql} from './quality-lineage-terminal-sql'");
    expect(read('adapters/quality-lineage-terminal-sql.ts').match(/SELECT /g)).toHaveLength(1);
  });
});

describe('quality inspection evidence/metric head dedupe (r24)', () => {
  test('one shared head reproduces both inspection evidence SELECTs byte-for-byte', () => {
    expect(inspectionEvidenceSource(evidenceList)).toBe(`SELECT ${evidenceList}${evidenceJoin}`);
    expect(inspectionEvidenceSource(evidenceDetail)).toBe(`SELECT ${evidenceDetail}${evidenceJoin}`);
    for (const projection of [evidenceList, evidenceDetail]) expect(inspectionEvidenceSource(projection)).toMatch(/^SELECT /);
  });
  test('the same head reproduces the pass-rate metric SELECT byte-for-byte with binds unchanged', () => {
    const sql = inspectionMetricSource(metricProjection, metricTail);
    expect(sql).toBe(metricBefore);
    expect(sql.match(/\?/g)).toHaveLength(5);
    expect(metricBefore.match(/\?/g)).toHaveLength(5);
  });
  test('both evidence readers and the metric reader share the head with no inline copies', () => {
    const evidence = read('adapters/executive-main-evidence-sources.ts');
    const metric = read('adapters/executive-metric-projections.ts');
    expect(evidence.match(/inspectionEvidenceSource\(/g)).toHaveLength(2);
    expect(metric.match(/inspectionMetricSource\(/g)).toHaveLength(1);
    expect(evidence).not.toContain(`SELECT ${evidenceList}`);
    expect(evidence).not.toContain('SELECT i.id,i.hold_id,i.result,i.evidence,i.inspected_by');
    expect(metric).not.toContain('FROM inventory_quality_inspections i JOIN inventory_quality_holds h ON h.id=i.hold_id');
    expect(metric).toContain('import { inspectionMetricSource, evidenceHead } from "./inspection-evidence-source-sql"');
    expect(read('adapters/inspection-evidence-source-sql.ts').match(/SELECT /g)).toHaveLength(1);
  });
});

describe('quality read-tail r24 protected families', () => {
  test('the new helpers are read-only; locks, native and r1-r23 helpers stay untouched', () => {
    for (const name of ['quality-lineage-terminal-sql', 'inspection-evidence-source-sql']) {
      expect(read(`adapters/${name}.ts`)).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    }
    expect(read('adapters/purchase-sources.ts')).toContain('qualityPurchaseSourceReadLocks');
    expect(read('adapters/attachment-read-sources.ts')).toContain('LOCK TABLE inventory_quality_holds,inventory_quality_specs');
    expect(read('adapters/sales-package-sources.ts')).toContain('LOCK TABLE inventory_quality_holds,inventory_quality_inspections,inventory_quality_disposition_events,inventory_quality_cancellations,inventory_quality_specs,inventory_quality_spec_metrics IN SHARE MODE');
    expect(read('adapters/attachments-policy-facts.ts')).toContain('descriptorTargets');
    expect(read('adapters/attachments-service-facts.ts')).toContain('UPDATE ${descriptor.table} SET version=version+1');
    expect(read('adapters/customer_return_hold_statement.rs')).toContain('customer_return_hold_statements');
    expect(read('adapters/customer_return_hold_http.rs')).toContain('customer_return_hold');
    const counts: Record<string, number> = {
      'quality-work-scope-sql': 1, 'native-source-wrapper-sql': 0, 'inspection-evidence-source-sql': 1,
      'quality-lineage-recursion-sql': 1, 'quality-child-agg-sql': 1,
      'quality-receipt-guard-sql': 1, 'quality-source-rows-sql': 1, 'quality-scope-sql': 1,
      'quality-alert-recipient-sql': 0, 'quality-attachment-scope-read-sql': 1, 'quality-id-source-sql': 1,
      'quality-attachment-source-select-sql': 1, 'quality-return-anchor-sql': 0,
      'quality-obligation-fact-arm-sql': 0, 'quality-lineage-terminal-sql': 1, 'quality-source-arm-head': 1,
    };
    for (const [name, count] of Object.entries(counts)) {
      expect(read(`adapters/${name}.ts`).match(/SELECT /g) ?? []).toHaveLength(count);
    }
  });
});

describe('quality read-tail r23 retirement check', () => {
  test('retirement:ts-statements is strictly lower than the BEFORE=63 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/INSERT INTO|DELETE FROM|UPDATE [a-z_]+ SET|SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(63);
    expect(total).toBe(25);
  });
});
