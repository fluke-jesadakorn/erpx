import {z} from "zod";
export const companyAcceptanceSubmitSchema = z.object({
  reason: z.string().trim().min(3).max(2000),
});
export const companyAcceptanceDecisionSchema = z.object({
  expectedVersion: z.number().int().positive(),
  decision: z.enum(["accept", "reject"]),
  reason: z.string().trim().min(3).max(2000),
});
export const companyAcceptanceRevokeSchema = z.object({
  expectedVersion: z.number().int().positive(),
  reason: z.string().trim().min(3).max(2000),
});
export type AcceptanceRow = {
  company_id: string;
  status: "draft" | "submitted" | "rejected" | "accepted" | "revoked";
  version: number;
  requested_by: string;
  reviewed_by: string | null;
  requested_at: number;
  reviewed_at: number | null;
  reason: string;
  review_reason: string | null;
  updated_at: number;
};
export type EventRow = { id: string; action: string; actor_id: string; version: number; reason: string; created_at: number };
export type Check = { id: string; label: string; ready: boolean; owner: string; detail: string };
