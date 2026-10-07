import "server-only";
import { runtimeEnv } from "@/server/modules/runtime/public/runtime";
import { readWorkspacePreference } from "@/server/modules/work_projection/public/workspace-preferences";
export async function savedWorkspaceForPresentation(userId: string) {
  return readWorkspacePreference((await runtimeEnv()).CONTROL_DB, userId);
}
