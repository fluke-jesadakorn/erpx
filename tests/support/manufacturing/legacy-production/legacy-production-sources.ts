import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';

/**
 * Test-only fixture sources for the retired TypeScript material-issue and
 * independent-QC acceptance writers.
 *
 * The native Manufacturing order command owns these lot, cost, move and
 * genealogy effects in production. The PostgreSQL integration oracle still
 * needs their exact pre-cutover writes, so they live here instead of in
 * `@erpx/server-manufacturing/adapters/production-command-sources`.
 */
export function legacyProductionCommandSources(db: SqlDatabase) {
  return {
    prepareReservationConsumption(id: string | number | null) {
      return db
        .prepare(
          "UPDATE manufacturing_material_reservations SET consumed_milli=quantity_milli WHERE id=?",
        )
        .bind(id);
    },
    prepareIssueMove(value: string, id: string, warehouse_id: string | number | null, product_id: string | number | null, next: number, value2: number, value3: number, entryDate: string, userId: string, now: number) {
      return db.prepare("INSERT INTO manufacturing_inventory_moves(id,order_id,warehouse_id,product_id,phase,source_order_version,quantity_milli,value_minor,movement_date,created_by,created_at) VALUES(?,?,?,?,'issue',?,?,?,?,?,?)")
        .bind(value, id, warehouse_id, product_id, next, value2, value3, entryDate, userId, now);
    },
    prepareMaterialCost(material: number, id: string) {
      return db
        .prepare(
          "UPDATE manufacturing_orders SET material_cost_minor=? WHERE id=?",
        )
        .bind(material, id);
    },
    prepareOutputMove(value: string, id: string, warehouse_id: string | number | null, output_product_id: string | number | null, next: number, output: number, total: number, entryDate: string, userId: string, now: number) {
      return db.prepare("INSERT INTO manufacturing_inventory_moves(id,order_id,warehouse_id,product_id,phase,source_order_version,quantity_milli,value_minor,movement_date,created_by,created_at) VALUES(?,?,?,?,'output',?,?,?,?,?,?)")
        .bind(value, id, warehouse_id, output_product_id, next, output, total, entryDate, userId, now);
    },
    prepareCostSnapshot(value: string, id: string, next: number, material_cost_minor: string | number | null, labor_cost_minor: string | number | null, overhead_cost_minor: string | number | null, total: number, output: number, now: number) {
      return db
        .prepare(
          "INSERT INTO manufacturing_cost_snapshots VALUES(?,?,?,?,?,?,?,?,?)",
        )
        .bind(
          value,
          id,
          next,
          material_cost_minor,
          labor_cost_minor,
          overhead_cost_minor,
          total,
          output,
          now,
        );
    },
    prepareProductGenealogy(value: string, id: string, product_id: string | number | null, output_product_id: string | number | null, value2: number, output: number, next: number, now: number) {
      return db
        .prepare(
          "INSERT INTO manufacturing_genealogy VALUES(?,?,?,?,?,?,?,?)",
        )
        .bind(
          value,
          id,
          product_id,
          output_product_id,
          value2,
          output,
          next,
          now,
        );
    },
  };
}
