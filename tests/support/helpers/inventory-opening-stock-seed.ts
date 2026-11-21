import 'server-only';
import {decimalToMinor} from '@erpx/feature-finance/lib/finance-money';
import type {PostgresDatabase} from '@erpx/server-runtime/adapters/runtime/postgres';

/**
 * Seed-only declarative opening stock.
 *
 * The TypeScript stock-count writer is retired; fictional development seeds no
 * longer create a Count/receipt/review chain just to establish demo stock. They
 * write the exact warehouse/product quantity and value the counted opening
 * would have posted. This is not production code and must never run in a
 * request path.
 */
export type OpeningStockLine = {productId: string; countedQuantity: string; unitCost: string};

export async function seedOpeningStock(
  database: PostgresDatabase,
  warehouseId: string,
  lines: readonly OpeningStockLine[],
) {
  if (!warehouseId || !lines.length) throw new Error('OPENING_STOCK_INPUT_REQUIRED');
  for (const line of lines) {
    const quantity = Number(line.countedQuantity) * 1000;
    const value = decimalToMinor(line.unitCost) * Number(line.countedQuantity);
    if (
      !line.productId ||
      !Number.isSafeInteger(quantity) ||
      quantity < 0 ||
      !Number.isSafeInteger(value) ||
      value < 0
    ) {
      throw new Error('OPENING_STOCK_INVALID');
    }
    await database
      .prepare(
        'INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version) VALUES(?,?,?,?,1) ON CONFLICT(warehouse_id,product_id) DO UPDATE SET quantity_milli=excluded.quantity_milli,value_minor=excluded.value_minor,version=commerce_stock.version+1',
      )
      .bind(warehouseId, line.productId, quantity, value)
      .run();
  }
  return {warehouseId, lines: lines.length};
}
