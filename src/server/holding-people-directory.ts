import "server-only";

import { hasPermission } from "@/lib/permissions";
import type { WorkContext } from "./authorization";
import { ApiError } from "./http";
import { requirePermission } from "./iam-permissions";
import { holdingBindingBySlug, holdingResources } from "./runtime";
import type { SqlDatabase } from "./runtime/types";
import { directoryAssignmentActive, readArchivedPeopleDirectory, readDirectorySelection, readHoldingPeopleDirectory, type DirectorySelection } from "./people/directory-repository";

export type HoldingDirectoryCompany = {
  id: string;
  slug: string;
  name: string;
};

export type HoldingDirectoryNode = {
  id: string;
  companyId: string;
  kind: "company" | "headquarters" | "branch" | "department" | "team";
  slug: string;
  name: string;
  parentId: string | null;
  branchId: string | null;
};

export type HoldingDirectoryLevel = {
  id: string;
  companyId: string;
  code: string;
  name: string;
  number: number | null;
};

export type HoldingDirectoryPosition = {
  id: string;
  companyId: string;
  branchId: string | null;
  departmentId: string | null;
  title: string;
  levelId: string;
  reportsToPositionId: string | null;
  isCeo: boolean;
};

export type HoldingDirectoryPerson = {
  /** Stable tree/list key: one person can have multiple current engagements. */
  id: string;
  personId: string;
  userId?: string | null;
  engagementId: string;
  companyId: string;
  branchId: string | null;
  departmentId: string | null;
  positionId: string | null;
  legalName: string;
  jobTitle: string;
  workEmail: string | null;
  workPhone: string | null;
};

export type HoldingPeopleDirectory = {
  companies: HoldingDirectoryCompany[];
  nodes: HoldingDirectoryNode[];
  levels: HoldingDirectoryLevel[];
  positions: HoldingDirectoryPosition[];
  people: HoldingDirectoryPerson[];
};

async function authorizedDirectoryDatabase(actor: WorkContext): Promise<SqlDatabase> {
  const binding = await holdingBindingBySlug(actor.holdingSlug);
  if (binding.id !== actor.holdingId || actor.scope.holdingId !== actor.holdingId || actor.scope.companyId !== actor.companyId)
    throw new ApiError(403, "HOLDING_SCOPE_REQUIRED", "Select an authorized Holding workspace.");

  const { write } = await holdingResources(actor.holdingId);
  const authorized = await directoryAssignmentActive(write, actor);
  if (!authorized)
    throw new ApiError(403, "HOLDING_DIRECTORY_FORBIDDEN", "Your work assignment is no longer active in this Holding.");
  return write;
}

/** Public read model. Never pass profile, account, or compensation rows to a client. */
export async function holdingPeopleDirectory(actor: WorkContext): Promise<HoldingPeopleDirectory> {
  return readHoldingPeopleDirectory(await authorizedDirectoryDatabase(actor));
}

/** Archived profiles remain an HR view within the actor's selected Company scope. */
export async function archivedPeopleDirectory(actor: WorkContext): Promise<HoldingDirectoryPerson[]> {
  requirePermission(actor, "people.manage");
  return readArchivedPeopleDirectory(await authorizedDirectoryDatabase(actor), actor);
}

/** Detail requests verify the fresh assignment and read only their selected item. */
export async function holdingDirectorySelection(actor: WorkContext, selection: DirectorySelection, archived = false) {
  return readDirectorySelection(await authorizedDirectoryDatabase(actor), actor, selection,
    archived && hasPermission(actor, "people.manage"));
}
