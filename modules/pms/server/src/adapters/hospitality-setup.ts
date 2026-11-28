import 'server-only';
import { holdingResources } from "../../../../../platform/runtime/server/src/public/runtime";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { nativePmsReadAll, PMS_RD_HOSPITALITY_SETUP_PROPERTIES } from "./native-pms-read";
import { loadPms } from "../public/pms";
import { pmsAccountCategory, pmsAccountCode } from "@erpx/finance-server/public/hospitality-accounting";
import { createHospitalitySetup, type PmsSetupRepository, type PmsSetupRow } from "@erpx/pms-server/application/hospitality-setup";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { pmsHospitalitySetupOrganizationFacts } from "@erpx/organization-server/public/pms-hospitality-setup-facts";
import { pmsHospitalitySetupCommerceFacts } from "@erpx/commerce-server/public/pms-hospitality-setup-facts";
import { pmsHospitalitySetupPosFacts } from "@erpx/pos-server/public/pms-hospitality-setup-facts";
import { pmsHospitalitySetupIdentityAccessFacts } from "@erpx/identity-access-server/public/pms-hospitality-setup-facts";
import { pmsHospitalitySetupFinanceFacts } from "@erpx/finance-server/public/pms-hospitality-setup-facts";
function repository(db: SqlDatabase): PmsSetupRepository {
    const organization = pmsHospitalitySetupOrganizationFacts(db);
    const commerce = pmsHospitalitySetupCommerceFacts(db);
    const pos = pmsHospitalitySetupPosFacts(db);
    const identity_access = pmsHospitalitySetupIdentityAccessFacts(db);
    const finance = pmsHospitalitySetupFinanceFacts(db);
    return {
        branches: facts => organization.branches(facts),
        properties: (facts: {
            company: string;
            branch: string;
        }) => nativePmsReadAll<PmsSetupRow>(PMS_RD_HOSPITALITY_SETUP_PROPERTIES, [facts.company, facts.branch], db).then(r => r.results),
        commerce: facts => commerce.commerce(facts),
        settings: facts => pos.settings(facts),
        warehouses: facts => commerce.warehouses(facts),
        registers: facts => pos.registers(facts),
        roles: facts => identity_access.roles(facts),
        periods: facts => finance.periods(facts),
        charts: facts => finance.charts(facts),
        accounts: facts => finance.accounts(facts),
        products: facts => commerce.products(facts),
        tables: facts => pos.tables(facts),
        stock: facts => commerce.stock(facts),
        companyName: facts => organization.companyName(facts), hotel: (actor, id) => loadPms(db, actor, id)
    };
}
const application = createHospitalitySetup({ repository: async (actor) => repository((await holdingResources(actor.holdingId)).write), clock: { now: () => Date.now() }, error: (status, code, message) => new ApiError(status, code, message), accountCode: pmsAccountCode, accountCategory: pmsAccountCategory, environment: () => process.env.NODE_ENV, roomChargePilot: () => process.env.ERPX_POS_ROOM_CHARGE_PILOT_ENABLED });
export const hospitalitySetup = application.hospitalitySetup;
export type HospitalitySetup = Awaited<ReturnType<typeof hospitalitySetup>>;
