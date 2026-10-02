import { measureReadPhase } from "./runtime/read-metrics";
import "server-only";

import { cache } from "react";
import {
  buildWorkspaceHoldingChoice,
  type WorkspaceChoice,
  type WorkspaceChoiceAssignment,
  type WorkspaceChoiceNode,
} from "@/lib/workspace-choice";
import { currentSession } from "./authorization";
import { iamRoles } from "./iam-store";
import {
  activeHoldingBindings,
  holdingResources,
  type HoldingBinding,
} from "./runtime";

type AssignmentRow = {
  role: string;
  company_id: string;
  company_name: string;
  scope_kind: "holding" | "branch" | "department";
  scope_node_id: string | null;
};

type NodeRow = {
  id: string;
  kind: "branch" | "department";
  slug: string;
  display_name: string;
  company_id: string;
  branch_id: string | null;
  branch_slug: string | null;
};

async function businessWorkspaceChoice(
  holding: HoldingBinding,
  userId: string,
  assignmentId?: string,
) {
  const { write } = await holdingResources(holding.id);
  const now = Date.now();
  const rows = (
    await write
      .prepare(
        `SELECT a.role,a.company_id,c.display_name AS company_name,a.scope_kind,a.scope_node_id
         FROM assignments a
         JOIN organization_nodes c ON c.id=a.company_id AND c.kind='company' AND c.status='active'
         LEFT JOIN organization_nodes n ON n.id=a.scope_node_id
         WHERE a.user_id=? AND a.revoked_at IS NULL
           ${assignmentId ? "AND a.id=?" : ""}
           AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
           AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
           AND a.valid_from<=? AND a.valid_until>?
           AND (
             a.scope_kind='holding'
             OR (
               n.company_id=a.company_id AND n.status='active'
               AND n.kind=a.scope_kind
               AND (
                 n.branch_id IS NULL
                 OR n.kind='branch'
                 OR EXISTS(
                   SELECT 1 FROM organization_nodes b
                   WHERE b.id=n.branch_id AND b.kind='branch'
                     AND b.company_id=a.company_id AND b.status='active'
                 )
               )
             )
           )
         ORDER BY c.display_name,c.id,a.scope_kind,a.scope_node_id,a.id`,
      )
      .bind(userId, ...(assignmentId ? [assignmentId] : []), now, now)
      .all<AssignmentRow>()
  ).results;

  const definitions = new Map(await Promise.all([...new Set(rows.map(row=>row.company_id))].map(async companyId =>
    [companyId, await iamRoles(write,companyId,rows.filter(row=>row.company_id===companyId).map(row=>row.role))] as const)));
  const assignments: WorkspaceChoiceAssignment[] = [];
  for (const row of rows) {
    const role = definitions.get(row.company_id)?.get(row.role);
    if (!role?.scopes.includes(row.scope_kind)) continue;
    assignments.push({
      companyId: row.company_id,
      companyName: row.company_name,
      scopeKind: row.scope_kind,
      scopeNodeId: row.scope_node_id,
    });
  }
  const companyIds = [...new Set(assignments.map((row) => row.companyId))];
  if (!companyIds.length) return null;
  const placeholders = companyIds.map(() => "?").join(",");
  const nodeRows = (
    await write
      .prepare(
        `SELECT n.id,n.kind,n.slug,n.display_name,n.company_id,n.branch_id,b.slug AS branch_slug
         FROM organization_nodes n
         LEFT JOIN organization_nodes b
           ON b.id=n.branch_id AND b.kind='branch' AND b.status='active'
         WHERE n.company_id IN (${placeholders})
           AND n.kind IN ('branch','department') AND n.status='active'
         ORDER BY n.display_name,n.id`,
      )
      .bind(...companyIds)
      .all<NodeRow>()
  ).results;
  const nodes: WorkspaceChoiceNode[] = nodeRows.map((row) => ({
    id: row.id,
    kind: row.kind,
    slug: row.slug,
    label: row.display_name,
    companyId: row.company_id,
    branchId: row.kind === "department" ? row.branch_id : null,
    branchSlug: row.kind === "department" ? row.branch_slug : null,
  }));
  const choice = buildWorkspaceHoldingChoice(
    holding.slug,
    holding.displayName,
    assignments,
    nodes,
  );
  if (!choice) return null;
  const profiles = (await write.prepare("SELECT scope_kind,scope_id,primary_publication_id FROM organization_brand_profiles WHERE scope_kind='holding' OR company_id IN (" + placeholders + ")")
    .bind(...companyIds).all<{ scope_kind: "holding" | "branch"; scope_id: string; primary_publication_id: string | null }>()).results;
  const short = (label: string) => label.trim().split(/\s+/u).filter(Boolean).slice(0, 2).map((word) => Array.from(word)[0] ?? "").join("").toLocaleUpperCase().slice(0, 3) || "—";
  const present = (label: string, kind: "holding" | "branch", id: string) => {
    const publication = profiles.find((profile) => profile.scope_kind === kind && profile.scope_id === id)?.primary_publication_id;
    return { label, initials: short(label), ...(publication ? { primaryUrl: `/api/public/holdings/${encodeURIComponent(holding.slug)}/media-publications/${encodeURIComponent(publication)}/content` } : {}) };
  };
  return {
    ...choice,
    brand: present(choice.label, "holding", holding.id),
    companies: choice.companies.map((company) => ({ ...company, branches: company.branches.map((branch) => ({ ...branch, brand: present(branch.label, "branch", branch.id) })) })),
  };
}

export const workspaceChoices = cache(() => measureReadPhase("workspace_choices",readWorkspaceChoices));
/** Scope choices to the role the user just chose, without selecting a workspace. */
export async function workspaceChoicesForAssignment(holdingId: string, assignmentId: string): Promise<WorkspaceChoice[]> {
  const [{ session }, holdings] = await Promise.all([currentSession(), activeHoldingBindings()]);
  const holding = holdings.find(item => item.id === holdingId);
  if (!holding) return [];
  const choice = await businessWorkspaceChoice(holding, session.user.id, assignmentId);
  return choice ? [choice] : [];
}
async function readWorkspaceChoices(): Promise<WorkspaceChoice[]> {
  const { session } = await currentSession();
  const business = (
    await Promise.all(
      (await activeHoldingBindings()).map((holding) =>
        businessWorkspaceChoice(holding, session.user.id),
      ),
    )
  )
    .filter((choice): choice is NonNullable<typeof choice> => !!choice)
    .sort(
      (a, b) =>
        a.label.localeCompare(b.label, undefined, { sensitivity: "base" }) ||
        a.holdingSlug.localeCompare(b.holdingSlug),
    );
  return [...business];
}
