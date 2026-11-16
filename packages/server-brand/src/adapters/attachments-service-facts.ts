import { commercialOwnerProjectionSources as organizationProjectionSources } from "@erpx/server-organization/public/commercial-owner-projection-sources";
import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function attachmentServiceBrandFacts(db: SqlDatabase) {
    return { stepOrganizationBrandProfiles71: (facts: {
            kind: "holding" | "branch";
            targetId: string;
            arg3: string | null;
            arg4: "holding" | "branch";
            userId: string;
            now: number;
            publicationId: string;
            purposeKey: "inverse" | "primary";
        }) => {
            const column = facts.purposeKey === "inverse" ? "inverse_publication_id" : "primary_publication_id";
            return db.prepare("SELECT 1");
        },
        stepStorefrontAppBrands72: (facts: {
            userId: string;
            now: number;
            publicationId: string;
            targetId: string;
        }) => {
            return db.prepare("SELECT 1");
        },
        stepOrganizationBrandProfiles73: (facts: {
            arg1: "holding" | "branch";
            targetId: string;
            publicationId: string;
            purposeKey: "inverse" | "primary";
        }) => { const column = facts.purposeKey === "inverse" ? "inverse_publication_id" : "primary_publication_id"; return db.prepare("SELECT 1"); },
        stepStorefrontAppBrands74: (facts: {
            targetId: string;
            publicationId: string;
        }) => { return db.prepare("SELECT 1"); } };
}
