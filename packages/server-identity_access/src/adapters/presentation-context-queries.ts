import "server-only";
import { headers } from "next/headers";
import { ApiError } from "@erpx/server-runtime/public/http";
import type { IamActor } from "../domain/iam-actor";
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
import {
  createNativeIamAssignmentsClient,
  nativeIamAssignmentsAvailable,
  nativeIamAssignmentsConfiguration,
  nativeIamAssignmentsFailure,
  NativeIamAssignmentsError,
  type NativeIamPageContext,
} from "../public/native-iam-assignments";

type IamActorLike = {
  userId: string;
  holdingId: string | null;
  companyId: string;
  branchId: string | null;
  departmentId: string | null;
  assignmentId: string;
  role: string;
  scopeKind: string;
  platform: boolean;
  authority?: { revision: number; assignmentVersion: number };
  iam?: { permissions: readonly string[] };
};

function nativeIamPageContext(
  actor: IamActorLike,
  holdingSlug: string,
): NativeIamPageContext {
  if (!actor.holdingId || !actor.authority || !actor.iam || actor.platform)
    throw new NativeIamAssignmentsError("NATIVE_IAM_ASSIGNMENTS_CONTEXT_CHANGED");
  return {
    userId: actor.userId,
    holdingId: actor.holdingId,
    holdingSlug,
    companyId: actor.companyId,
    branchId: actor.branchId,
    departmentId: actor.departmentId,
    assignmentId: actor.assignmentId,
    role: actor.role,
    assignmentScope: actor.scopeKind,
    authority: actor.authority,
    iam: { permissions: actor.iam.permissions },
  };
}

async function nativeAssignmentRows(actor: IamActorLike, holdingSlug: string) {
  if (
    !nativeIamAssignmentsAvailable() ||
    actor.platform ||
    actor.scopeKind !== "holding" ||
    !holdingSlug
  )
    return undefined;
  const raw = process.env.ERPX_NATIVE_IDENTITY_ACCESS_CONFIG;
  if (raw === undefined) return undefined;
  const page = nativeIamPageContext(actor, holdingSlug);
  const client = createNativeIamAssignmentsClient(nativeIamAssignmentsConfiguration(raw));
  return client.workspace(await headers(), page, {
    holdingSlug,
    expected: {
      companyId: page.companyId,
      assignmentId: page.assignmentId,
      revision: page.authority.revision,
      assignmentVersion: page.authority.assignmentVersion,
    },
  });
}

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
    iamOverview: async (actor: IamActor, self = false) => {
      const data = await iamOverview(db, actor, self);
      if (self) return data;
      try {
        const native = await nativeAssignmentRows(actor, holdingSlug ?? "");
        return native ? { ...data, assignments: native.assignments } : data;
      } catch (error) {
        const failure = nativeIamAssignmentsFailure(error);
        if (failure) throw new ApiError(failure.status, failure.code, failure.message);
        throw error;
      }
    },
    peopleTemplatesData: (...args: ReadArguments<typeof peopleTemplatesData>) => peopleTemplatesData(db, ...args),
  } };
}
