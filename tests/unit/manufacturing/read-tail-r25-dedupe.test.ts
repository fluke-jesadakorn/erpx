import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import {
  manufacturingJsonbAggSql,
  manufacturingOperationAggSql,
  manufacturingSpecificationLineAggSql,
  manufacturingSpecificationProductsAggSql,
} from '../../../packages/server-manufacturing/src/adapters/manufacturing-read-sql-fragments';

const srcRoot = resolve(__dirname, '../../../packages/server-manufacturing/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const S = 'MANUFACTURING_PRODUCT_SOURCES';

/** The six pre-r25 jsonb_agg definitions were byte-identical apart from the
 * projected expression, the ORDER BY column, the FROM source and the optional
 * trailing clause. Originals were captured verbatim from HEAD; the shared
 * helper concatenates its four arguments, so whole-string byte identity is the
 * oracle. `where` carries its own leading space or comma so even the comma-scoped
 * production aggregate re-emits its original text. */
const FAMILY: readonly {
  args: readonly [string, string, string, string?];
  original: string;
}[] = [
  {
    args: ['to_jsonb(operation)', 'operation.id', 'manufacturing_specification_operations operation', ' WHERE operation.specification_id=specification.id'],
    original: '(SELECT jsonb_agg(to_jsonb(operation) ORDER BY operation.id) FROM manufacturing_specification_operations operation WHERE operation.specification_id=specification.id)',
  },
  {
    args: ['to_jsonb(operation)', 'operation.id', 'manufacturing_operation_events operation', ' WHERE operation.order_id=record.id'],
    original: '(SELECT jsonb_agg(to_jsonb(operation) ORDER BY operation.id) FROM manufacturing_operation_events operation WHERE operation.order_id=record.id)',
  },
  {
    args: ['to_jsonb(operation)', 'operation.id', 'manufacturing_specification_operations operation', ' WHERE operation.specification_id=s.id'],
    original: '(SELECT jsonb_agg(to_jsonb(operation) ORDER BY operation.id) FROM manufacturing_specification_operations operation WHERE operation.specification_id=s.id)',
  },
  {
    args: ['to_jsonb(line)', 'line.id', 'manufacturing_specification_lines line', ' WHERE line.specification_id=specification.id'],
    original: '(SELECT jsonb_agg(to_jsonb(line) ORDER BY line.id) FROM manufacturing_specification_lines line WHERE line.specification_id=specification.id)',
  },
  {
    args: ['to_jsonb(line)', 'line.id', 'manufacturing_specification_lines line', ' WHERE line.specification_id=s.id'],
    original: '(SELECT jsonb_agg(to_jsonb(line) ORDER BY line.id) FROM manufacturing_specification_lines line WHERE line.specification_id=s.id)',
  },
  {
    args: ['to_jsonb(product)', 'product.id', `(${S}) product`, ` WHERE product.company_id=specification.company_id AND (product.id=specification.output_product_id OR product.id IN(SELECT product_id FROM manufacturing_specification_lines WHERE specification_id=specification.id))`],
    original: `(SELECT jsonb_agg(to_jsonb(product) ORDER BY product.id) FROM (${S}) product WHERE product.company_id=specification.company_id AND (product.id=specification.output_product_id OR product.id IN(SELECT product_id FROM manufacturing_specification_lines WHERE specification_id=specification.id)))`,
  },
  {
    args: ['to_jsonb(product)', 'product.id', `(${S}) product`, ` WHERE product.company_id=s.company_id AND (product.id=s.output_product_id OR product.id IN(SELECT product_id FROM manufacturing_specification_lines WHERE specification_id=s.id))`],
    original: `(SELECT jsonb_agg(to_jsonb(product) ORDER BY product.id) FROM (${S}) product WHERE product.company_id=s.company_id AND (product.id=s.output_product_id OR product.id IN(SELECT product_id FROM manufacturing_specification_lines WHERE specification_id=s.id)))`,
  },
  {
    args: ['to_jsonb(reservation)', 'reservation.id', 'manufacturing_material_reservations reservation', ' WHERE reservation.order_id=record.id'],
    original: '(SELECT jsonb_agg(to_jsonb(reservation) ORDER BY reservation.id) FROM manufacturing_material_reservations reservation WHERE reservation.order_id=record.id)',
  },
  {
    args: ['to_jsonb(genealogy)', 'genealogy.id', 'manufacturing_genealogy genealogy', ' WHERE genealogy.order_id=record.id'],
    original: '(SELECT jsonb_agg(to_jsonb(genealogy) ORDER BY genealogy.id) FROM manufacturing_genealogy genealogy WHERE genealogy.order_id=record.id)',
  },
  {
    args: ['to_jsonb(movement)', 'movement.id', 'manufacturing_cost_movements movement', ' WHERE movement.order_id=record.id AND movement.company_id=record.company_id'],
    original: '(SELECT jsonb_agg(to_jsonb(movement) ORDER BY movement.id) FROM manufacturing_cost_movements movement WHERE movement.order_id=record.id AND movement.company_id=record.company_id)',
  },
  {
    args: ["jsonb_build_object('posting',to_jsonb(posting),'profile',to_jsonb(profile))", 'posting.id', 'manufacturing_order_postings posting LEFT JOIN manufacturing_accounting_profiles profile ON profile.id=posting.profile_id AND profile.company_id=posting.company_id', ' WHERE posting.order_id=record.id AND posting.company_id=record.company_id'],
    original: "(SELECT jsonb_agg(jsonb_build_object('posting',to_jsonb(posting),'profile',to_jsonb(profile)) ORDER BY posting.id) FROM manufacturing_order_postings posting LEFT JOIN manufacturing_accounting_profiles profile ON profile.id=posting.profile_id AND profile.company_id=posting.company_id WHERE posting.order_id=record.id AND posting.company_id=record.company_id)",
  },
  {
    args: ['to_jsonb(o)', 'o.id', 'manufacturing_orders o'],
    original: '(SELECT jsonb_agg(to_jsonb(o) ORDER BY o.id) FROM manufacturing_orders o)',
  },
  {
    args: ['to_jsonb(p)', 'p.id', 'manufacturing_order_postings p'],
    original: '(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id) FROM manufacturing_order_postings p)',
  },
  {
    args: ['to_jsonb(m)', 'm.id', 'manufacturing_cost_movements m'],
    original: '(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.id) FROM manufacturing_cost_movements m)',
  },
  {
    args: ['to_jsonb(p)', 'p.id', 'manufacturing_accounting_profiles p', ',scope WHERE p.company_id=scope.company_id'],
    original: '(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id) FROM manufacturing_accounting_profiles p,scope WHERE p.company_id=scope.company_id)',
  },
];

describe('manufacturing read-tail r25 jsonb_agg helper unification', () => {
  it('renders all six former definitions byte-identically through one helper', () => {
    for (const { args, original } of FAMILY) {
      expect(manufacturingJsonbAggSql(...args)).toBe(original);
      expect(original).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
    expect(manufacturingOperationAggSql('manufacturing_specification_operations operation', 'WHERE operation.specification_id=specification.id')).toBe(FAMILY[0].original);
    expect(manufacturingOperationAggSql('manufacturing_operation_events operation', 'WHERE operation.order_id=record.id')).toBe(FAMILY[1].original);
    expect(manufacturingSpecificationLineAggSql('specification')).toBe(FAMILY[3].original);
    expect(manufacturingSpecificationProductsAggSql('s', S)).toBe(FAMILY[6].original);
  });

  it('defines one text-only helper and routes all six definitions through it', () => {
    const fragments = src('manufacturing-read-sql-fragments.ts');
    expect(fragments.match(/export const manufacturingJsonbAggSql/g) ?? []).toHaveLength(1);
    expect(fragments.split('manufacturingJsonbAggSql(').length - 1).toBe(3);
    expect(fragments.match(/\bSELECT\b/g)).toHaveLength(9); // r26 adds the shared EXISTS probe and folds two local guards into it.
    expect(fragments).not.toMatch(/prepare\(|\.run\(|\.all\(|\.first\(/);
    const attachment = src('attachment-read-sources.ts');
    expect(attachment).toContain('manufacturingJsonbAggSql,');
    expect(attachment.split('manufacturingJsonbAggSql(').length - 1).toBe(2);
    expect(attachment).toContain('const manufacturingOrderLineageAgg=');
    const postings = src('posting-setup-sources.ts');
    expect(postings).toContain('manufacturingJsonbAggSql,manufacturingScopedCte');
    expect(postings.split('manufacturingJsonbAggSql(').length - 1).toBe(1);
    expect(postings).toContain('const productionAgg=');
    for (const { original } of FAMILY.slice(7)) {
      expect(attachment).not.toContain(original);
      expect(postings).not.toContain(original);
    }
  });

  it('leaves the r4/r11/r15-r24 helpers, locks and FOR UPDATE witness untouched', () => {
    const fragments = src('manufacturing-read-sql-fragments.ts');
    expect(fragments).toContain('export const manufacturingRowsSelect');
    expect(fragments).toContain('export const manufacturingIdSelect');
    expect(fragments).toContain('export const manufacturingAttachmentVersionGuard');
    expect(src('manufacturing-read-tail-r15-sql.ts')).toContain('export const manufacturingOrderJoinSelect');
    expect(src('manufacturing-read-tail-r16-sql.ts')).toContain('export const manufacturingPurchaseRecordSelect');
    expect(src('manufacturing-read-tail-r17-sql.ts')).toContain('export const manufacturingSettlementFactSelect');
    expect(src('manufacturing-read-tail-r18-sql.ts')).toContain('export const manufacturingPurchaseRecordBranchSelect');
    expect(src('manufacturing-read-tail-r19-sql.ts')).toContain('export const manufacturingAccountingProfileSelect');
    expect(src('manufacturing-read-tail-r20-sql.ts')).toContain('export const manufacturingExistsGuard');
    expect(src('manufacturing-read-tail-r21-sql.ts')).toContain('export const manufacturingAttachmentImageSelect');
    expect(src('manufacturing-read-tail-r22-sql.ts')).toContain('export const manufacturingOrderDetailSelect');
    expect(src('manufacturing-read-tail-r23-sql.ts')).toContain('export const manufacturingStockFootprintSelect');
    expect(src('manufacturing-read-tail-r24-sql.ts')).toContain('export const manufacturingPurchaseScopedSelect');
    expect(src('purchases.ts')).toContain('LOCK TABLE planning_scenarios,planning_recommendations,planning_scenario_events');
    expect(src('production-command-sources.ts')).toContain('WHERE id=? FOR UPDATE');
    expect(src('attachment-read-sources.ts')).toContain('LOCK TABLE manufacturing_material_reservations,manufacturing_genealogy,manufacturing_cost_snapshots,manufacturing_operation_events');
  });

  it('keeps retirement:ts-statements strictly below the r24 BEFORE=70 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(51); // r24 left 70; r25 70->65; r26 EXISTS guards -> 60; r27 journal projection family 60->58; r28 cost-allocation probe 58->57; r29 scoped EXISTS probe 57->56; r30 scope CTE family 56->55; r31 allowed-select head family 55->53; r32 metric breakdown head 53->52; r34 r15/r23 head fold 52->51.
    expect(total).toBeLessThan(60);
  });
});
