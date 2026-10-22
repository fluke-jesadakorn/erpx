import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function posCoreFilesFacts(db: SqlDatabase) {
    return {
        readAttachmentSets1: (facts: {
            companyId: string;
            fact1: Array<string>;
            placeholders: string;
        }) => db.prepare(`SELECT id,purpose_key,component_kind FROM attachment_sets WHERE company_id=? AND id IN (${facts.placeholders})`).bind(facts.companyId, ...facts.fact1).all<{
            id: string;
            purpose_key: string;
            component_kind: string;
        }>()
    };
}
