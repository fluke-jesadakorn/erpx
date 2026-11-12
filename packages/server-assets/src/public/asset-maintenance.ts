export { assetWorkspace, assetDetail, createAsset } from "@erpx/server-assets/adapters/assets";
/** Compatibility while callers move to the Maintenance owner's public API. */
export { maintenanceDowntimeEvidenceRows, maintenanceWorkspace, maintenanceDetail, maintenanceWork } from "@erpx/server-maintenance/public/asset-maintenance";
export type { MaintenanceDowntimeEvidenceRow } from "@erpx/server-maintenance/public/asset-maintenance";
