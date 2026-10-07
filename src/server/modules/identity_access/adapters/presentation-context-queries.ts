import "server-only";
import { iamContext } from "@/server/modules/identity_access/public/iam-context";
import { organizationAccessData } from "@/server/modules/organization/public/organization-access";
import { peopleLifecycleData } from "@/server/modules/people_payroll/public/people-lifecycle";
import { directoryAutomationData } from "@/server/modules/identity_access/public/directory-operations";
import { technologyData } from "@/server/modules/identity_access/public/technology";
import { gpoRulesOverview } from "@/server/modules/identity_access/public/gpo-rules";
import { peopleAccessData } from "@/server/modules/people_payroll/public/people-access";
import { readManagement } from "@/server/modules/identity_access/public/user-management";
import { iamOverview } from "@/server/modules/identity_access/public/iam-service";
import { peopleTemplatesData } from "@/server/modules/people_payroll/public/people-templates";

type ReadArguments<F extends (...args: never[]) => unknown> = Parameters<F> extends [unknown, ...infer Rest] ? Rest : never;

export async function iamPresentationQueries(holdingSlug: string | null) {
  const { db, actor } = await iamContext(holdingSlug);
  return { actor, queries: {
    organizationAccessData: (...args: ReadArguments<typeof organizationAccessData>) => organizationAccessData(db, ...args),
    peopleLifecycleData: (...args: ReadArguments<typeof peopleLifecycleData>) => peopleLifecycleData(db, ...args),
    directoryAutomationData: (...args: ReadArguments<typeof directoryAutomationData>) => directoryAutomationData(db, ...args),
    technologyData: (...args: ReadArguments<typeof technologyData>) => technologyData(db, ...args),
    gpoRulesOverview: (...args: ReadArguments<typeof gpoRulesOverview>) => gpoRulesOverview(db, ...args),
    peopleAccessData: (...args: ReadArguments<typeof peopleAccessData>) => peopleAccessData(db, ...args),
    readManagement: (...args: ReadArguments<typeof readManagement>) => readManagement(db, ...args),
    iamOverview: (...args: ReadArguments<typeof iamOverview>) => iamOverview(db, ...args),
    peopleTemplatesData: (...args: ReadArguments<typeof peopleTemplatesData>) => peopleTemplatesData(db, ...args),
  } };
}
