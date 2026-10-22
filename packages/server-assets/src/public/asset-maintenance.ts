export { assetWorkspace, assetDetail, createAsset, changeAsset } from "@erpx/server-assets/adapters/assets";
/** Compatibility while callers move to the Maintenance owner's public API. */
export { maintenanceDowntimeEvidenceRows, maintenanceWorkspace, maintenanceDetail, createMaintenanceWork, changeMaintenanceWork, maintenanceWork } from "@erpx/server-maintenance/public/asset-maintenance";
export type { MaintenanceDowntimeEvidenceRow } from "@erpx/server-maintenance/public/asset-maintenance";
