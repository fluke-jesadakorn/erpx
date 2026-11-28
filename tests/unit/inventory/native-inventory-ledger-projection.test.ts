import { describe, it, expect, vi } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
import { inventoryLedgerProjectionSql } from '../../../modules/inventory/server/src/adapters/inventory-read-projections';
import { commercialOwnerProjectionSources } from '../../../modules/inventory/server/src/adapters/commercial-owner-projection-sources';
import { hospitalityOwnerProjectionSources } from '../../../modules/inventory/server/src/adapters/hospitality-owner-projection-sources';

const src = (file: string) => readFileSync(resolve(__dirname, '../../../modules/inventory/server/src/adapters', file), 'utf8');

describe('inventory owner projection dedupe', () => {
  it('exposes one canonical ledger fixed SQL and shares it with every native join source', () => {
    expect(inventoryLedgerProjectionSql).toBe(
      'SELECT "id","warehouse_id","product_id","quantity_milli","value_minor","created_at","movement_date","source_type","source_id","source_number" FROM inventory_ledger',
    );
    expect(commercialOwnerProjectionSources.inventory_ledger).toBe(inventoryLedgerProjectionSql);
    expect(hospitalityOwnerProjectionSources.inventory_ledger).toBe(inventoryLedgerProjectionSql);
  });

  it('routes the canonical Stock Count status read through the native registry for every count-source caller', () => {
    const callers = [
      'initial-physical-stock-count-source.ts',
      'manufacturing-initial-physical-stock-count-source.ts',
      'pos-initial-physical-stock-count-source.ts',
      'count-purchases.ts',
    ];
    for (const file of callers) {
      const text = src(file);
      expect(text).toContain('countStatus');
      expect(text).not.toContain('SELECT status FROM inventory_counts WHERE id=? AND company_id=?');
    }
    const projections = src('inventory-read-projections.ts');
    expect(projections).not.toContain('inventoryCountStatusSql');
    expect(projections).toContain('inventory_rd_count_status');
  });

  it('routes every purchase replay probe through the native registry', () => {
    const replay = "SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND record_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?";
    const callers = ['count-purchases.ts', 'landed-cost-purchases.ts', 'transfer-purchases.ts'];
    for (const file of callers) {
      const text = src(file);
      expect(text).toContain('purchaseReplayExists');
      expect(text).not.toContain(replay);
    }
    expect(src('inventory-read-projections.ts')).not.toContain('SELECT 1 FROM command_receipts');
  });

  it('routes every initial-count command replay probe through the native registry', () => {
    const replay = "SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?";
    const callers = [
      'initial-physical-stock-count-source.ts',
      'pos-initial-physical-stock-count-source.ts',
      'manufacturing-initial-physical-stock-count-source.ts',
    ];
    for (const file of callers) {
      const text = src(file);
      expect(text).toContain('commandReplayExists');
      expect(text).not.toContain(replay);
    }
    expect(src('inventory-read-projections.ts')).not.toContain('SELECT 1 FROM command_receipts');
  });
});
