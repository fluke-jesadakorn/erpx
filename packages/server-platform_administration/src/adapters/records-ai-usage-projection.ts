import { platformOwnerProjectionSources } from "@erpx/server-runtime/public/platform-owner-projection-sources";
import {platformReadHeads} from "./platform-read-heads";
export const recordsExecutiveUsageSql = `${platformReadHeads.recordsUsageProjection}${platformOwnerProjectionSources.executive_generation_jobs}) executive_generation_jobs`;
