import {workRoutinePresentation} from '@erpx/server-operations/public/work-routine-presentation';
import {projectPurchaseControls} from "@erpx/server-projects/public/purchases";
import "server-only";
import { staffPage } from "@erpx/server-work_projection/public/staff-page";
import type {StaffPageOptions} from '@erpx/server-work_projection/public/staff-page';
import { getProject } from "@erpx/server-operations/public/staff-tasks";
import { listTasks } from "@erpx/server-operations/public/staff-tasks";
import { listProjectMilestones } from "@erpx/server-projects/public/project-milestones";
import { listProjectWorkLogs } from "@erpx/server-projects/public/project-actuals";
import { projectMemberIds } from "@erpx/server-projects/public/people-app-entrypoint-facts";
import { listTemplates } from "@erpx/server-operations/public/routines";
import { getTemplate } from "@erpx/server-operations/public/routines";
import { canEditTemplate } from "@erpx/server-operations/public/routines";
import { routineTemplateSchedules } from "@erpx/server-operations/public/people-app-entrypoint-facts";
import { getWorkCapabilities } from "@erpx/server-work_projection/public/work-capabilities";
import { taskDetail } from "@erpx/server-operations/public/staff-tasks";
import { taskSocialOrigin } from "@erpx/server-collaboration/public/people-app-entrypoint-facts";
import { readablePost } from "@erpx/server-collaboration/public/social-source";

type ReadArguments<F extends (...args: never[]) => unknown> = Parameters<F> extends [unknown, ...infer Rest] ? Rest : never;

export async function staffPresentationQueries(holdingSlug: string,presentation?:StaffPageOptions) {
  const { write, actor, base, options, projects, staff,projectPickerStrict } = await (presentation?staffPage(holdingSlug,presentation):staffPage(holdingSlug));
  return { actor, base, options, projects, staff, queries: {
    projectPurchases:()=>projectPickerStrict?projectPurchaseControls(write,actor,{strictProduct:true}):projectPurchaseControls(write,actor),
    routinePurchases:()=>workRoutinePresentation(write,actor),
    getProject: (...args: ReadArguments<typeof getProject>) => getProject(write, ...args),
    listTasks: (...args: ReadArguments<typeof listTasks>) => listTasks(write, ...args),
    listProjectMilestones: (...args: ReadArguments<typeof listProjectMilestones>) => listProjectMilestones(write, ...args),
    listProjectWorkLogs: (...args: ReadArguments<typeof listProjectWorkLogs>) => listProjectWorkLogs(write, ...args),
    projectMemberIds: (...args: ReadArguments<typeof projectMemberIds>) => projectMemberIds(write, ...args),
    listTemplates: (...args: ReadArguments<typeof listTemplates>) => listTemplates(write, ...args),
    getTemplate: (...args: ReadArguments<typeof getTemplate>) => getTemplate(write, ...args),
    canEditTemplate: (...args: ReadArguments<typeof canEditTemplate>) => canEditTemplate(write, ...args),
    routineTemplateSchedules: (...args: ReadArguments<typeof routineTemplateSchedules>) => routineTemplateSchedules(write, ...args),
    getWorkCapabilities: (...args: ReadArguments<typeof getWorkCapabilities>) => getWorkCapabilities(write, ...args),
    taskDetail: (...args: ReadArguments<typeof taskDetail>) => taskDetail(write, ...args),
    taskSocialOrigin: (...args: ReadArguments<typeof taskSocialOrigin>) => taskSocialOrigin(write, ...args),
    readablePost: (...args: ReadArguments<typeof readablePost>) => readablePost(write, ...args),
  } };
}
