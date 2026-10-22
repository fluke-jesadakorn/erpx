import { z } from "zod";

export const brandIdentitySchema = z.object({
  identity: z.object({ shortName: z.string().trim().min(1).max(40), tagline: z.string().trim().max(160).default("") }).strict(),
  visual: z.object({ accentColor: z.string().regex(/^#[a-fA-F0-9]{6}$/).transform((value) => value.toLowerCase()), logoPublicationId: z.string().min(1).nullable() }).strict(),
}).strict();
export type BrandIdentity = z.infer<typeof brandIdentitySchema>;
const reason = z.string().trim().min(3).max(4000);
const version = z.number().int().positive();
export const brandDraftInput = z.object({ id: z.string().min(1).optional(), expectedVersion: version.optional(), expectedPublishedVersion: z.number().int().nonnegative(), identity: brandIdentitySchema, reason });
export const brandPublishInput = z.object({ expectedVersion: version, expectedPublishedVersion: z.number().int().nonnegative(), reason });
export const brandRollbackInput = z.object({ publishedId: z.string().min(1), expectedPublishedVersion: version, reason });
export const brandCategoryLockInput = z.object({ category: z.enum(["identity", "visual"]), locked: z.boolean(), expectedVersion: z.number().int().nonnegative(), expectedPublishedVersion: version, reason });
export const brandCancelInput = z.object({ expectedVersion: version, reason });

export type BrandDraft = { id: string; company_id: string; based_revision: number; identity_json: string; status: string; version: number; created_by: string; updated_by: string; updated_at: number };
export type BrandPublished = { id: string; company_id: string; revision: number; identity_json: string; draft_id: string; draft_version: number; created_by: string; published_by: string; published_at: number };
export type ActiveBrandIdentity = BrandPublished & { active_version: number; identity: BrandIdentity };
export type BrandCategoryLock = { category: "identity" | "visual"; locked: boolean; approved_snapshot_json: string; version: number; reason: string; updated_by: string; updated_at: number };
