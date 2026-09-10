import "server-only";
import { requireWorkContext, publicScope } from "./authorization";
import { workspaceBase } from "@/lib/workspace";
import { holdingResources } from "./runtime";
import { staffSettings } from "./staff-settings";
import { listProjects } from "./staff-tasks";
import { activeStaff } from "./staff-policy";
import { ApiError } from "./http";
export async function staffPage(holdingSlug: string) {
  const actor = await requireWorkContext(holdingSlug),
    { write } = await holdingResources(actor.holdingId);
  const [options, projects, staff] = await Promise.all([
    staffSettings(write, actor),
    listProjects(write, actor),
    activeStaff(write, actor.companyId, actor.userId).catch((e) => {
      if (e instanceof ApiError) return null;
      throw e;
    }),
  ]);
  return {
    actor,
    write,
    base: workspaceBase(publicScope(actor)),
    options,
    projects,
    staff,
  };
}
