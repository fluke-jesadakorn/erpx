import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { QUALITY_HOLD_SOURCE_SELECT, inventoryExecutiveOwnerProjections } from '../../../modules/quality/server/src/adapters/inventory-executive-owner-projections';
import { inventoryQualitySnapshotSources } from '../../../modules/quality/server/src/adapters/inventory-quality-snapshot-sources';
import { collaborationNotificationSnapshotSources } from '../../../modules/quality/server/src/adapters/collaboration-notification-snapshot-sources';

describe('quality hold projection dedupe', () => {
  it('owns one fixed SELECT over the full hold row', () => {
    expect(QUALITY_HOLD_SOURCE_SELECT.match(/SELECT /g)).toHaveLength(1);
    expect(QUALITY_HOLD_SOURCE_SELECT).toContain('FROM inventory_quality_holds');
    expect(QUALITY_HOLD_SOURCE_SELECT).toContain('"disposition_quantity_milli"');
    expect(QUALITY_HOLD_SOURCE_SELECT).toContain('"quantity_scope"');
  });

  it('composes every owner projection fact from the single canonical SELECT', () => {
    expect(inventoryExecutiveOwnerProjections.inventory_quality_holds).toBe(QUALITY_HOLD_SOURCE_SELECT);
    expect(inventoryQualitySnapshotSources().inventory_counts_inventory_quality_holds_from)
      .toBe(`FROM (${QUALITY_HOLD_SOURCE_SELECT}) inventory_quality_holds`);
    const notifications = collaborationNotificationSnapshotSources();
    expect(notifications.quality_notifications_inventory_quality_holds_h_from)
      .toBe(`FROM (${QUALITY_HOLD_SOURCE_SELECT}) h`);
    expect(notifications.quality_notifications_inventory_quality_holds_h_join)
      .toBe(`JOIN (${QUALITY_HOLD_SOURCE_SELECT}) h`);
    expect(notifications.quality_notifications_inventory_quality_alerts_a_from)
      .toContain('FROM inventory_quality_alerts) a');
  });
});
