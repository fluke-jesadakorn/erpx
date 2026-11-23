import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { inventoryLotMovesSql } from '../../../packages/server-inventory/src/adapters/inventory-read-projections';
import { maintenanceSettlementStockSources } from '../../../packages/server-inventory/src/adapters/maintenance-settlement-sources';
import { salesInventorySettlementSources } from '../../../packages/server-inventory/src/adapters/sales-package-obligations';

describe('inventory lot-moves owner projection dedupe', () => {
  it('exposes one canonical fixed SQL and shares it with every settlement source', () => {
    expect(inventoryLotMovesSql).toBe('SELECT * FROM inventory_lot_moves');
    expect(maintenanceSettlementStockSources.moves).toBe(inventoryLotMovesSql);
    expect(salesInventorySettlementSources.lotMoves).toBe(inventoryLotMovesSql);
  });
});
