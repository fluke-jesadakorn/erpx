/** Compatibility entry point; legacy implementation belongs to identity_access. */
export {applyDirectoryReview,applyDirectorySnapshot,connectionById,connectionInput,disableConnection,drainDirectoryJobs,mappingInput,proposeConnectionActivation,proposeMapping,refreshDirectoryMember,routedConnection,saveConnection,synchronizeConnection,testConnection} from "@/server/modules/identity_access/adapters/iam-directories";
export type {ConnectionRow} from "@/server/modules/identity_access/adapters/iam-directories";
