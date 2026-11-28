//! Retained legacy Directory writer oracles. Production writers were retired;
//! these fixtures keep the exact old SQL/application behavior for integration
//! proofs only. Never import from product code.
export {
  connectionInput,
  mappingInput,
  saveConnection,
  testConnection,
  proposeConnectionActivation,
  disableConnection,
  applyDirectoryReview,
  proposeMapping,
  drainDirectoryJobs,
  refreshDirectoryMember,
} from './iam-directories';
export type { ConnectionRow } from './iam-directories';
export { applyDirectorySnapshot, synchronizeConnection } from './synchronization';
