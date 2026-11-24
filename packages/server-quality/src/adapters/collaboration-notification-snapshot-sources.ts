import 'server-only';
import {QUALITY_HOLD_SOURCE_SELECT} from './inventory-executive-owner-projections';
import {qualitySourceArmHead} from './quality-source-arm-head';
/** Closed, named source clauses for the supplied notification statement snapshot.
 * No table, SQL, predicate or alias is accepted from a caller. */
export function collaborationNotificationSnapshotSources(){return {
  "quality_notifications_inventory_quality_holds_h_from": `FROM (${QUALITY_HOLD_SOURCE_SELECT}) h`,
  "quality_notifications_inventory_quality_alerts_a_from": `FROM (${qualitySourceArmHead("\"id\", \"hold_id\", \"phase\", \"source_version\", \"due_date\", \"detected_at\", \"version\", \"acknowledged_by\", \"acknowledged_at\", \"reason\"","inventory_quality_alerts")}) a`,
  "quality_notifications_inventory_quality_holds_h_join": `JOIN (${QUALITY_HOLD_SOURCE_SELECT}) h`
} as const;}
