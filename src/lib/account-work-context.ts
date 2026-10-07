import type { CurrentWorkspaceChoice, WorkspaceChoice } from "./workspace-choice";
import type { WorkAssignment } from "@/server/modules/identity_access/domain/authorization";

/** Serializable recovery evidence for the page that remains open after a switch. */
export type AccountWorkContext = {
  user: { id: string; name: string; email: string };
  from: string;
  status: "ready" | "role-required" | "workspace-required" | "forbidden";
  sameWorkspace: boolean;
  canUseWorkspace: boolean;
  requiresWorkspaceSelection: boolean;
  choices: WorkspaceChoice[];
  current?: CurrentWorkspaceChoice;
  canManageErpx: boolean;
  assignments: WorkAssignment[];
  assignmentId: string | null;
  revision: number;
  context: { userId: string; companyId: string; assignmentId: string; revision: number } | null;
  problem?: { code: string; message: string };
};
