import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
import { holdEvidenceSource, inspectionEvidenceSource, inspectionMetricSource } from '../../../packages/server-quality/src/adapters/inspection-evidence-source-sql';
import { commercialOwnerProjectionSources as commerce } from '@erpx/server-commerce/public/commercial-owner-projection-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-quality/src');
const read = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');

// Byte oracles copied from the pre-dedupe r25 source. The hold-evidence head
// was an inline SELECT in executive-main-evidence-sources.ts; the inspection
// evidence/metric heads were already shared by the r24 helper. None changed.
const timeColumn = 'h.released_at';
const holdProjection = `h.id,h.id hold_id,${timeColumn} created_at,h.version reviewed_version,h.status result,
      p.name product_name,w.name warehouse_name`;
const holdHeadBefore = `SELECT ${holdProjection}
    FROM inventory_quality_holds h
    JOIN (${commerce.commerce_products}) p ON p.id=h.product_id AND p.company_id=h.company_id
    JOIN (${commerce.commerce_warehouses}) w ON w.id=h.warehouse_id AND w.company_id=h.company_id`;

const evidenceList = 'i.id,i.hold_id,i.result,i.created_at,i.reviewed_version,p.name product_name,w.name warehouse_name';
const evidenceDetail = 'i.id,i.hold_id,i.result,i.evidence,i.inspected_by,i.created_at,i.reviewed_version,\n      p.name product_name,w.name warehouse_name';
const evidenceJoin = `\n    FROM inventory_quality_inspections i
    JOIN inventory_quality_holds h ON h.id=i.hold_id
    JOIN (${commerce.commerce_products}) p ON p.id=h.product_id AND p.company_id=h.company_id
    JOIN (${commerce.commerce_warehouses}) w ON w.id=h.warehouse_id AND w.company_id=h.company_id`;
const metricLabel = "'Company-wide'";
const metricProjection = `${metricLabel} label,\n        count(*) FILTER (WHERE i.result='pass')::text numerator,count(*)::text denominator`;
const metricTail = '\n      WHERE h.company_id=? AND (?::text IS NULL OR h.branch_id=?) AND i.created_at>=? AND i.created_at<?\n      GROUP BY 1 ORDER BY 1';
const metricBefore = `SELECT ${metricLabel} label,
        count(*) FILTER (WHERE i.result='pass')::text numerator,count(*)::text denominator
      FROM inventory_quality_inspections i JOIN inventory_quality_holds h ON h.id=i.hold_id
      WHERE h.company_id=? AND (?::text IS NULL OR h.branch_id=?) AND i.created_at>=? AND i.created_at<?
      GROUP BY 1 ORDER BY 1`;

describe('quality hold-evidence full head dedupe (r25)', () => {
  test('one shared head reproduces the inline hold-evidence SELECT byte-for-byte', () => {
    expect(holdEvidenceSource(holdProjection)).toBe(holdHeadBefore);
    const stripped = holdHeadBefore
      .replace(`(${commerce.commerce_products})`, '(p)')
      .replace(`(${commerce.commerce_warehouses})`, '(w)');
    expect(stripped.match(/SELECT /g)).toHaveLength(1);
    expect(holdEvidenceSource(holdProjection).startsWith('SELECT ')).toBe(true);
    expect(holdEvidenceSource(holdProjection)).not.toContain('?');
    expect(holdEvidenceSource(holdProjection)).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });
  test('the same shared head keeps both inspection evidence/metric outputs byte-identical', () => {
    expect(inspectionEvidenceSource(evidenceList)).toBe(`SELECT ${evidenceList}${evidenceJoin}`);
    expect(inspectionEvidenceSource(evidenceDetail)).toBe(`SELECT ${evidenceDetail}${evidenceJoin}`);
    expect(inspectionMetricSource(metricProjection, metricTail)).toBe(metricBefore);
    expect(inspectionMetricSource(metricProjection, metricTail).match(/\?/g)).toHaveLength(5);
  });
  test('the helper emits one SELECT template shared by all three readers', () => {
    const head = read('adapters/inspection-evidence-source-sql.ts');
    expect(head.match(/SELECT /g)).toHaveLength(1);
    expect(head).toContain('=> `SELECT ${projection}${from}${tail}`');
    expect(head).not.toContain('?');
    expect(head).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });
  test('the hold-evidence reader uses the shared head with no residual inline copy', () => {
    const exec = read('adapters/executive-main-evidence-sources.ts');
    expect(exec.match(/holdEvidenceSource\(/g)).toHaveLength(1);
    expect(exec).toContain('import { holdEvidenceSource, inspectionEvidenceSource, evidenceHead } from "./inspection-evidence-source-sql";');
    expect(exec).not.toContain('commerceProjectionSources');
    expect(exec).not.toContain('SELECT h.id,h.id hold_id,');
    expect(exec).not.toContain('FROM inventory_quality_holds h\n    JOIN (${commerceProjectionSources.commerce_products})');
    // the inspection parent lookup routes through the r26 exported evidence head
    expect(exec).toContain('evidenceHead(`i.hold_id`,');
  });
});

describe('quality read-tail r25 protected families', () => {
  test('locks, native and r1-r24 helpers stay untouched', () => {
    expect(read('adapters/purchase-sources.ts')).toContain('qualityPurchaseSourceReadLocks');
    expect(read('adapters/attachment-read-sources.ts')).toContain('LOCK TABLE inventory_quality_holds,inventory_quality_specs');
    expect(read('adapters/sales-package-sources.ts')).toContain('LOCK TABLE inventory_quality_holds,inventory_quality_inspections,inventory_quality_disposition_events,inventory_quality_cancellations,inventory_quality_specs,inventory_quality_spec_metrics IN SHARE MODE');
    expect(read('adapters/attachments-policy-facts.ts')).toContain('descriptorTargets');
    expect(read('adapters/attachments-service-facts.ts')).toContain('UPDATE ${descriptor.table} SET version=version+1');
    expect(read('adapters/customer_return_hold_statement.rs')).toContain('customer_return_hold_statements');
    expect(read('adapters/customer_return_hold_http.rs')).toContain('customer_return_hold');
    const counts: Record<string, number> = {
      'quality-work-scope-sql': 1, 'native-source-wrapper-sql': 1, 'inspection-evidence-source-sql': 1,
      'quality-lineage-recursion-sql': 2, 'quality-child-agg-sql': 1,
      'quality-receipt-guard-sql': 1, 'quality-source-rows-sql': 1, 'quality-scope-sql': 1,
      'quality-alert-recipient-sql': 1, 'quality-attachment-scope-read-sql': 1, 'quality-id-source-sql': 1,
      'quality-attachment-source-select-sql': 1, 'quality-source-union-sql': 1, 'quality-return-anchor-sql': 0,
      'quality-obligation-fact-arm-sql': 0, 'quality-lineage-terminal-sql': 1, 'quality-source-arm-head': 1,
    };
    for (const [name, count] of Object.entries(counts)) {
      expect(read(`adapters/${name}.ts`).match(/SELECT /g) ?? []).toHaveLength(count);
    }
  });
});

describe('quality read-tail r25 dedupe retirement check', () => {
  test('retirement:ts-statements is strictly lower than the BEFORE=58 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/INSERT INTO|DELETE FROM|UPDATE [a-z_]+ SET|SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(58);
    expect(total).toBe(30);
  });
});
