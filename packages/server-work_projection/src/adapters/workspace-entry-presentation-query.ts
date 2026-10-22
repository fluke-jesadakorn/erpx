import "server-only";
import { runtimeEnv } from "../../../../packages/server-runtime/src/public/runtime";
import { readWorkspacePreference } from "@erpx/server-work_projection/public/workspace-preferences";
export async function savedWorkspaceForPresentation(userId: string) {
  return readWorkspacePreference((await runtimeEnv()).CONTROL_DB, userId);
}
