import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { inventoryLedgerProjectionSql } from '../../../packages/server-inventory/src/adapters/inventory-read-projections';
import { commercialOwnerProjectionSources } from '../../../packages/server-inventory/src/adapters/commercial-owner-projection-sources';
import { hospitalityOwnerProjectionSources } from '../../../packages/server-inventory/src/adapters/hospitality-owner-projection-sources';

describe('inventory ledger owner projection dedupe', () => {
  it('exposes one canonical fixed SQL and shares it with every native join source', () => {
    expect(inventoryLedgerProjectionSql).toBe(
      'SELECT "id","warehouse_id","product_id","quantity_milli","value_minor","created_at","movement_date","source_type","source_id","source_number" FROM inventory_ledger',
    );
    expect(commercialOwnerProjectionSources.inventory_ledger).toBe(inventoryLedgerProjectionSql);
    expect(hospitalityOwnerProjectionSources.inventory_ledger).toBe(inventoryLedgerProjectionSql);
  });
});
