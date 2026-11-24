import 'server-only';
import { finalRootOwnerProjectionSources as collaborationProjectionSources } from '@erpx/server-collaboration/public/final-root-owner-projection-sources';
import {operationsProjectSourceExists} from './project-source-exists';

/** Canonical social task-origin existence probe shared by the Operations cohort
 *  visibility and handover source readers. Only the row alias and outer task
 *  reference vary; each rendering is byte-identical to the retired inline literal. */
export const operationsSocialTaskOriginExists = (alias: string, task: string) =>
  operationsProjectSourceExists(collaborationProjectionSources.social_task_origins,alias,`${alias}.task_id=${task}`);
