import { z } from "zod";

export const createConnectionSchema = z.object({
  name: z.string().trim().min(1).max(80),
  scopeId: z.string().min(1).max(180),
  expiresDays: z.number().int().min(1).max(90).default(30),
}).strict();
export type ConnectionScope = { id: string; label: string; branchId: string | null; departmentId: string | null };
export type ExternalConnection = {
  id: string; name: string; role: string; scope: string;
  status: "active" | "expired" | "revoked" | "access_changed";
  createdAt: number; expiresAt: number; lastUsedAt: number | null;
};
export type ExternalConnections = { canCreate: boolean; scopes: ConnectionScope[]; connections: ExternalConnection[] };
