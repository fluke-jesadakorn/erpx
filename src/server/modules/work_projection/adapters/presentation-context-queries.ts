import {workRoutinePresentation} from '../../operations/public/work-routine-presentation';
import {projectPurchaseControls} from "../../projects/public/purchases";
import "server-only";
import { staffPage } from "@/server/modules/work_projection/public/staff-page";
import type {StaffPageOptions} from '@/server/modules/work_projection/public/staff-page';
import { getProject } from "@/server/modules/operations/public/staff-tasks";
import { listTasks } from "@/server/modules/operations/public/staff-tasks";
import { listProjectMilestones } from "@/server/modules/projects/public/project-milestones";
import { listProjectWorkLogs } from "@/server/modules/projects/public/project-actuals";
import { projectMemberIds } from "@/server/modules/projects/public/people-app-entrypoint-facts";
import { listTemplates } from "@/server/modules/operations/public/routines";
import { getTemplate } from "@/server/modules/operations/public/routines";
import { canEditTemplate } from "@/server/modules/operations/public/routines";
import { routineTemplateSchedules } from "@/server/modules/operations/public/people-app-entrypoint-facts";
import { getWorkCapabilities } from "@/server/modules/work_projection/public/work-capabilities";
import { taskDetail } from "@/server/modules/operations/public/staff-tasks";
import { taskSocialOrigin } from "@/server/modules/collaboration/public/people-app-entrypoint-facts";
import { readablePost } from "@/server/modules/collaboration/public/social-source";

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
