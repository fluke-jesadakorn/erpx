import { commercialOwnerProjectionSources as organizationProjectionSources } from "../../organization/public/commercial-owner-projection-sources";
import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
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
            return db.prepare(`INSERT INTO organization_brand_profiles(scope_kind,scope_id,company_id,document_mode,updated_by,updated_at,${column}) VALUES(?,?,?,?,?,?,?)
      ON CONFLICT(scope_kind,scope_id) DO UPDATE SET ${column}=excluded.${column},updated_by=excluded.updated_by,updated_at=excluded.updated_at`).bind(facts.kind, facts.targetId, facts.arg3, facts.arg4, facts.userId, facts.now, facts.publicationId);
        },
        stepStorefrontAppBrands72: (facts: {
            userId: string;
            now: number;
            publicationId: string;
            targetId: string;
        }) => {
            return db.prepare(`INSERT INTO storefront_app_brands(company_id,short_name,accent_color,updated_by,updated_at,logo_publication_id)
    SELECT id,left(display_name,40),'#3659c9',?,?,? FROM (${organizationProjectionSources.organization_nodes}) organization_nodes WHERE id=? AND kind='company' AND status='active'
    ON CONFLICT(company_id) DO UPDATE SET logo_publication_id=excluded.logo_publication_id,updated_by=excluded.updated_by,updated_at=excluded.updated_at`).bind(facts.userId, facts.now, facts.publicationId, facts.targetId);
        },
        stepOrganizationBrandProfiles73: (facts: {
            arg1: "holding" | "branch";
            targetId: string;
            publicationId: string;
            purposeKey: "inverse" | "primary";
        }) => { const column = facts.purposeKey === "inverse" ? "inverse_publication_id" : "primary_publication_id"; return db.prepare(`UPDATE organization_brand_profiles SET ${column}=NULL WHERE scope_kind=? AND scope_id=? AND ${column}=?`).bind(facts.arg1, facts.targetId, facts.publicationId); },
        stepStorefrontAppBrands74: (facts: {
            targetId: string;
            publicationId: string;
        }) => { return db.prepare("UPDATE storefront_app_brands SET logo_publication_id=NULL WHERE company_id=? AND logo_publication_id=?").bind(facts.targetId, facts.publicationId); } };
}
