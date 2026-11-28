import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import {legalSettingsEnabledSql} from './legal-read-tail-r9-sources';
export function pmsRolloutLegalFacts(db: SqlDatabase) {
    return {
        legal: (companyId: string) => db.prepare(legalSettingsEnabledSql).bind(companyId).first()
    };
}
