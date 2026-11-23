import { describe, it, expect, vi } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
import { inventoryLedgerProjectionSql, inventoryCountStatusSql } from '../../../packages/server-inventory/src/adapters/inventory-read-projections';
import { commercialOwnerProjectionSources } from '../../../packages/server-inventory/src/adapters/commercial-owner-projection-sources';
import { hospitalityOwnerProjectionSources } from '../../../packages/server-inventory/src/adapters/hospitality-owner-projection-sources';

const src = (file: string) => readFileSync(resolve(__dirname, '../../../packages/server-inventory/src/adapters', file), 'utf8');

describe('inventory owner projection dedupe', () => {
  it('exposes one canonical ledger fixed SQL and shares it with every native join source', () => {
    expect(inventoryLedgerProjectionSql).toBe(
      'SELECT "id","warehouse_id","product_id","quantity_milli","value_minor","created_at","movement_date","source_type","source_id","source_number" FROM inventory_ledger',
    );
    expect(commercialOwnerProjectionSources.inventory_ledger).toBe(inventoryLedgerProjectionSql);
    expect(hospitalityOwnerProjectionSources.inventory_ledger).toBe(inventoryLedgerProjectionSql);
  });

  it('shares one canonical Stock Count status SQL with every count-source caller', () => {
    expect(inventoryCountStatusSql).toBe('SELECT status FROM inventory_counts WHERE id=? AND company_id=?');
    const callers = [
      'initial-physical-stock-count-source.ts',
      'manufacturing-initial-physical-stock-count-source.ts',
      'pos-initial-physical-stock-count-source.ts',
      'count-purchases.ts',
    ];
    for (const file of callers) {
      const text = src(file);
      expect(text).toContain('inventoryCountStatusSql');
      expect(text).not.toContain('SELECT status FROM inventory_counts WHERE id=? AND company_id=?');
    }
  });
});
