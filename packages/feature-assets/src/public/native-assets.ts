import 'server-only';

/** Asset register reads execute only in the mounted native owner transport.
 * The deleted TypeScript purchase gate is not part of this path. */
export {
  nativeAssetDetail,
  nativeAssetWorkspace,
} from '../adapters/server/native-assets-page';
/** Retained names for callers outside this claim; these resolve directly to
 * the native transport, never to the deleted TypeScript register authority. */
export {
  nativeAssetDetail as ownedAssetDetail,
  nativeAssetWorkspace as ownedAssetWorkspace,
} from '../adapters/server/native-assets-page';
export {ownedMaintenanceDetail, ownedMaintenancePlanDetail, ownedMaintenancePlans, ownedMaintenanceWorkspace} from '../adapters/server/native-maintenance-owned';
