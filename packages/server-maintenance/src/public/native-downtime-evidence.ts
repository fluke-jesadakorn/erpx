/**
 * Public surface of the native downtime-evidence read transport. The owner
 * route `/maintenance/downtime-evidence` is the sole read authority; no owner
 * SQL remains in TypeScript and an absent configuration never falls back.
 */
export {
  nativeDowntimeEvidenceRows,
  nativeDowntimeReadConfigured,
  type NativeDowntimeQuery,
} from '../adapters/native-downtime-evidence';
export type { MaintenanceDowntimeEvidenceRow } from '@erpx/server-maintenance/domain/work';
