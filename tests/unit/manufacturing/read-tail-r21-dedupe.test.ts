import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { manufacturingAttachmentImageSelect } from '../../../packages/server-manufacturing/src/adapters/manufacturing-read-tail-r21-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-manufacturing/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

/** The three pre-dedupe attachment-image SELECT statements, captured verbatim
 * from HEAD. Whole-string byte identity is the oracle: no projection, source,
 * join, whitespace or bind may drift. `${...}` placeholders are literal text
 * because the helper only concatenates fragments; callers interpolate their own
 * source rows at runtime. */
const FAMILY: readonly { args: readonly [string, string, string]; original: string }[] = [
  {
    args: ["record.image", " FROM (${specificationRows})", ""],
    original: "SELECT record.image::text image FROM (${specificationRows}) record WHERE record.id=? AND record.company_id=?",
  },
  {
    args: ["jsonb_build_object('native',record.image,'specification',specification.image,'warehouse',to_jsonb(warehouse))", "\n FROM (${planningAttachmentScenarioSourceRows})", " LEFT JOIN (${specificationRows}) specification ON specification.id=record.specification_id AND specification.company_id=record.company_id LEFT JOIN (${manufacturingWarehouseSources}) warehouse ON warehouse.id=record.warehouse_id AND warehouse.company_id=record.company_id"],
    original: "SELECT jsonb_build_object('native',record.image,'specification',specification.image,'warehouse',to_jsonb(warehouse))::text image\n FROM (${planningAttachmentScenarioSourceRows}) record LEFT JOIN (${specificationRows}) specification ON specification.id=record.specification_id AND specification.company_id=record.company_id LEFT JOIN (${manufacturingWarehouseSources}) warehouse ON warehouse.id=record.warehouse_id AND warehouse.company_id=record.company_id WHERE record.id=? AND record.company_id=?",
  },
  {
    args: ["jsonb_build_object('order',to_jsonb(record),'specification',specification.image,'recommendation',recommendation.image,'warehouse',to_jsonb(warehouse),\n 'reservations',${manufacturingOrderLineageAgg(\"reservation\",\"manufacturing_material_reservations\")},\n 'genealogy',${manufacturingOrderLineageAgg(\"genealogy\",\"manufacturing_genealogy\")},\n 'costSnapshot',(SELECT to_jsonb(snapshot) FROM manufacturing_cost_snapshots snapshot WHERE snapshot.order_id=record.id),\n 'costMovements',${manufacturingOrderLineageAgg(\"movement\",\"manufacturing_cost_movements\",\" AND movement.company_id=record.company_id\")},\n 'postings',(SELECT jsonb_agg(jsonb_build_object('posting',to_jsonb(posting),'profile',to_jsonb(profile)) ORDER BY posting.id) FROM manufacturing_order_postings posting LEFT JOIN manufacturing_accounting_profiles profile ON profile.id=posting.profile_id AND profile.company_id=posting.company_id WHERE posting.order_id=record.id AND posting.company_id=record.company_id),\n 'operations',${manufacturingOperationAggSql(\"manufacturing_operation_events operation\",\"WHERE operation.order_id=record.id\")})", "\n FROM manufacturing_orders", " LEFT JOIN (${specificationRows}) specification ON specification.id=record.specification_id AND specification.company_id=record.company_id\n LEFT JOIN (${planningAttachmentRecommendationSourceRows}) recommendation ON recommendation.id=record.recommendation_id AND recommendation.company_id=record.company_id LEFT JOIN (${manufacturingWarehouseSources}) warehouse ON warehouse.id=record.warehouse_id AND warehouse.company_id=record.company_id"],
    original: "SELECT jsonb_build_object('order',to_jsonb(record),'specification',specification.image,'recommendation',recommendation.image,'warehouse',to_jsonb(warehouse),\n 'reservations',${manufacturingOrderLineageAgg(\"reservation\",\"manufacturing_material_reservations\")},\n 'genealogy',${manufacturingOrderLineageAgg(\"genealogy\",\"manufacturing_genealogy\")},\n 'costSnapshot',(SELECT to_jsonb(snapshot) FROM manufacturing_cost_snapshots snapshot WHERE snapshot.order_id=record.id),\n 'costMovements',${manufacturingOrderLineageAgg(\"movement\",\"manufacturing_cost_movements\",\" AND movement.company_id=record.company_id\")},\n 'postings',(SELECT jsonb_agg(jsonb_build_object('posting',to_jsonb(posting),'profile',to_jsonb(profile)) ORDER BY posting.id) FROM manufacturing_order_postings posting LEFT JOIN manufacturing_accounting_profiles profile ON profile.id=posting.profile_id AND profile.company_id=posting.company_id WHERE posting.order_id=record.id AND posting.company_id=record.company_id),\n 'operations',${manufacturingOperationAggSql(\"manufacturing_operation_events operation\",\"WHERE operation.order_id=record.id\")})::text image\n FROM manufacturing_orders record LEFT JOIN (${specificationRows}) specification ON specification.id=record.specification_id AND specification.company_id=record.company_id\n LEFT JOIN (${planningAttachmentRecommendationSourceRows}) recommendation ON recommendation.id=record.recommendation_id AND recommendation.company_id=record.company_id LEFT JOIN (${manufacturingWarehouseSources}) warehouse ON warehouse.id=record.warehouse_id AND warehouse.company_id=record.company_id WHERE record.id=? AND record.company_id=?",
  },
];

describe('manufacturing read-tail r21 attachment-image select family', () => {
  it('renders the shared image select byte-identically to all three originals and stays read-only', () => {
    for (const { args, original } of FAMILY) {
      expect(manufacturingAttachmentImageSelect(...args)).toBe(original);
      expect(original).toContain('SELECT ');
      expect(original).toContain('::text image');
      expect(original).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  it('defines one text-only helper, routes all three callers and leaves no residual inline SQL', () => {
    const helper = src('manufacturing-read-tail-r21-sql.ts');
    expect(helper.match(/export const manufacturingAttachmentImageSelect/g) ?? []).toHaveLength(1);
    expect(helper).not.toMatch(/prepare\(|\.run\(|\.all\(|\.first\(/);
    const caller = src('attachment-read-sources.ts');
    expect(caller.split('manufacturingAttachmentImageSelect(').length - 1).toBe(3);
    expect(caller).toContain("from './manufacturing-read-tail-r21-sql'");
    for (const { original } of FAMILY) expect(caller).not.toContain(original);
  });

  it('leaves the r11/r4/r15-r20 helpers, locks and FOR UPDATE witness untouched', () => {
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
    expect(src('package-obligations.ts')).toContain('LOCK TABLE manufacturing_material_reservations,manufacturing_genealogy');
    expect(src('finance-record-sources.ts')).toContain('LOCK TABLE planning_scenarios');
    expect(src('attachment-read-sources.ts')).toContain('LOCK TABLE manufacturing_material_reservations,manufacturing_genealogy,manufacturing_cost_snapshots,manufacturing_operation_events');
    expect(src('purchases.ts')).toContain('LOCK TABLE planning_scenarios,planning_recommendations,planning_scenario_events');
    expect(src('production-command-sources.ts')).toContain('WHERE id=? FOR UPDATE');
    expect(src('manufacturing-read-tail-r21-sql.ts')).not.toMatch(/LOCK\s+TABLE|FOR\s+UPDATE|FOR\s+SHARE/i);
  });

  it('keeps retirement:ts-statements strictly below the r20 BEFORE=79 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThanOrEqual(77); // r20 left 79; r21 lowered 79->77 (three image selects -> one helper); r22 lowers 77->73.
    expect(total).toBeLessThan(79);
  });
});
