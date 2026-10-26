import "server-only";
import { peopleAccessSnapshot, peopleRelationshipsSnapshot } from "@erpx/server-identity_access/public/people-access";
import { iamContext } from "@erpx/server-identity_access/public/iam-context";
import { organizationAccessData } from "@erpx/server-organization/public/organization-access";
import { peopleLifecycleData } from "@erpx/server-people_payroll/public/people-lifecycle";
import { directoryAutomationData } from "@erpx/server-identity_access/public/directory-operations";
import { technologyData } from "@erpx/server-identity_access/public/technology";
import { gpoRulesOverview } from "@erpx/server-identity_access/public/gpo-rules";
import { peopleAccessData } from "@erpx/server-people_payroll/public/people-access";
import { readManagement } from "@erpx/server-identity_access/public/user-management";

import {iamOverview} from '@erpx/server-identity_access/public/iam-service-reads';
import { peopleTemplatesData } from "@erpx/server-people_payroll/public/people-templates";

type ReadArguments<F extends (...args: never[]) => unknown> = Parameters<F> extends [unknown, ...infer Rest] ? Rest : never;

export async function iamPresentationQueries(holdingSlug: string | null) {
  const { db, actor } = await iamContext(holdingSlug);
  return { actor, queries: {
    peopleRelationshipsSnapshot: (...args: ReadArguments<typeof peopleRelationshipsSnapshot>) => peopleRelationshipsSnapshot(db, ...args),
    peopleAccessSnapshot: (...args: ReadArguments<typeof peopleAccessSnapshot>) => peopleAccessSnapshot(db, ...args),
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
