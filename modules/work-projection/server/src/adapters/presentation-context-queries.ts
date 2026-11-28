import {projectPurchaseControls} from "@erpx/projects-server/public/purchases";
import "server-only";
import { staffPage } from "@erpx/work-projection-server/public/staff-page";
import type {StaffPageOptions} from '@erpx/work-projection-server/public/staff-page';
import {getProject} from '@erpx/operations-server/public/staff-tasks';
import {listTasks} from '@erpx/operations-server/public/staff-tasks';
import { nativeProjectMilestones } from "@erpx/projects-server/public/project-milestones";
import { listProjectWorkLogs } from "@erpx/projects-server/public/project-actuals";
import { projectMemberIds } from "@erpx/projects-server/public/people-app-entrypoint-facts";
import { getWorkCapabilities } from "@erpx/work-projection-server/public/work-capabilities";
import {taskDetail} from '@erpx/operations-server/public/staff-tasks';
import { taskSocialOrigin } from "@erpx/collaboration-server/public/people-app-entrypoint-facts";
import {readablePost} from '@erpx/collaboration-server/public/social-source';

type ReadArguments<F extends (...args: never[]) => unknown> = Parameters<F> extends [unknown, ...infer Rest] ? Rest : never;

export async function staffPresentationQueries(holdingSlug: string,presentation?:StaffPageOptions) {
  const { write, actor, base, options, projects, staff,projectPickerStrict } = await (presentation?staffPage(holdingSlug,presentation):staffPage(holdingSlug));
  return { actor, base, options, projects, staff, queries: {
    projectPurchases:()=>projectPickerStrict?projectPurchaseControls(write,actor,{strictProduct:true}):projectPurchaseControls(write,actor),
    getProject: (...args: ReadArguments<typeof getProject>) => getProject(write, ...args),
    listTasks: (...args: ReadArguments<typeof listTasks>) => listTasks(write, ...args),
    listProjectMilestones: (...args: ReadArguments<typeof nativeProjectMilestones>) => nativeProjectMilestones(write, ...args),
    listProjectWorkLogs: (...args: ReadArguments<typeof listProjectWorkLogs>) => listProjectWorkLogs(write, ...args),
    projectMemberIds: (...args: ReadArguments<typeof projectMemberIds>) => projectMemberIds(write, ...args),
    getWorkCapabilities: (...args: ReadArguments<typeof getWorkCapabilities>) => getWorkCapabilities(write, ...args),
    taskDetail: (...args: ReadArguments<typeof taskDetail>) => taskDetail(write, ...args),
    taskSocialOrigin: (...args: ReadArguments<typeof taskSocialOrigin>) => taskSocialOrigin(write, ...args),
    readablePost: (...args: ReadArguments<typeof readablePost>) => readablePost(write, ...args),
  } };
}
