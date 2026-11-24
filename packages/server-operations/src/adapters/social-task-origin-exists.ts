import 'server-only';
import { finalRootOwnerProjectionSources as collaborationProjectionSources } from '@erpx/server-collaboration/public/final-root-owner-projection-sources';

/** Canonical social task-origin existence probe shared by the Operations cohort
 *  visibility and handover source readers. Only the row alias and outer task
 *  reference vary; each rendering is byte-identical to the retired inline literal. */
export const operationsSocialTaskOriginExists = (alias: string, task: string) =>
  `EXISTS(SELECT 1 FROM (${collaborationProjectionSources.social_task_origins}) ${alias} WHERE ${alias}.task_id=${task})`;
