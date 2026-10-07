export { assetWorkspace, assetDetail, createAsset, changeAsset } from "../adapters/assets";
/** Compatibility while callers move to the Maintenance owner's public API. */
export { maintenanceDowntimeEvidenceRows, maintenanceWorkspace, maintenanceDetail, createMaintenanceWork, changeMaintenanceWork, maintenanceWork } from "../../maintenance/public/asset-maintenance";
export type { MaintenanceDowntimeEvidenceRow } from "../../maintenance/public/asset-maintenance";
