import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import {all,one} from '@erpx/server-runtime/public/runtime/sql';
import {ORG_NODE_DISPLAY_NAME_SOURCE} from './org-sources';
import {branchOptionalId} from './organization-read-tail-sources-r42';
export function pmsHospitalitySetupOrganizationFacts(db: SqlDatabase) {
    return {
        branches: (facts: {
            company: string;
            branchId: string | null;
            branchId2: string | null;
        }) => all(db, branchOptionalId('id,display_name name', ' ORDER BY display_name'), facts.company, facts.branchId, facts.branchId2),
        companyName: (facts: {
            company: string;
        }) => one(db, ORG_NODE_DISPLAY_NAME_SOURCE, facts.company)
    };
}
