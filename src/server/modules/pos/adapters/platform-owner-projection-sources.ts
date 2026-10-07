import "server-only";
/** Closed owner projections for Platform review and operational snapshots in the caller transaction. */
export const platformOwnerProjectionSources = Object.freeze({
  pos_settings: "SELECT \"company_id\",\"currency\" FROM pos_settings",
} as const);
