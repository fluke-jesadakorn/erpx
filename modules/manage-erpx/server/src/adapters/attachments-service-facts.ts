import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import {betaPolicyLimitSql} from './beta-policy-limit-sql';
export const betaPolicyStorageBytesSql = betaPolicyLimitSql('storage_bytes');
export function attachmentServiceManageErpxFacts(db: SqlDatabase) { return { readBetaPolicy20: (facts: {
        companyId: string;
    }) => { return db.prepare(betaPolicyStorageBytesSql).bind(facts.companyId).first<{
        storage_bytes: number;
    }>(); } }; }
