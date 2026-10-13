import { z } from "zod";
import type { AccessPackageDefinition } from "@/lib/people_payroll/people-lifecycle";
const id = z.string().min(1).max(180);
export const provisioningInput = z
  .object({
    engagementIds: z
      .array(id)
      .min(1)
      .max(100)
      .refine(
        (v) => new Set(v).size === v.length,
        "Choose each employee once.",
      ),
    sourceKind: z.enum(["position", "supplemental"]).default("position"),
    packageId: id.nullable().default(null),
    revision: z.number().int().positive().nullable().default(null),
    exclude: z.array(z.string().min(1).max(500)).max(100).default([]),
    validUntil: z.number().int().positive(),
    restoreAccess: z.boolean().default(false),
    revokeOnly: z.boolean().default(false),
    revokeApplicationIds: z.array(id).max(100).default([]),
    reason: z.string().trim().min(3).max(1000),
  })
  .refine(
    (v) => !v.revokeOnly || v.revokeApplicationIds.length > 0,
    "Choose the applied sources to withdraw.",
  )
  .refine(
    (v) => v.sourceKind !== "supplemental" || (!!v.packageId && !!v.revision),
    "Choose a published package for additional access.",
  );
export type ProvisioningInput = z.infer<typeof provisioningInput>;
export type AppliedPackage = {
  id: string;
  person_id: string;
  engagement_id: string;
  position_id: string | null;
  package_id: string;
  package_revision: number;
  source_kind: "position" | "supplemental";
  definition: AccessPackageDefinition;
  scope: { exclude?: string[] };
  valid_until: number;
  ended_at: number | null;
  active: boolean;
};
export type ProvisioningPreview = {
  token: string;
  employees: {
    id: string;
    name: string;
    packageName: string;
    revision: number;
    added: string[];
    removed: string[];
    retained: string[];
    reviews: number;
  }[];
  warnings: string[];
};
export type TechnologyData = {
  applications: AppliedPackage[];
  operations: {
    id: string;
    status: string;
    version: number;
    last_error: string | null;
    steps: { id: string; status: string; label: string }[];
    created_at: number;
    reviewsPending: number;
  }[];
};
