/** Compatibility entry; its legacy implementation is tracked in ownership.json. */
export { connectionInput, connectionById, routedConnection, saveConnection, testConnection, proposeConnectionActivation, disableConnection, applyDirectorySnapshot, synchronizeConnection, mappingInput, proposeMapping, applyDirectoryReview, drainDirectoryJobs, refreshDirectoryMember } from "@/server/modules/identity_access/adapters/iam-directories";
export type { ConnectionRow } from "@/server/modules/identity_access/adapters/iam-directories";
