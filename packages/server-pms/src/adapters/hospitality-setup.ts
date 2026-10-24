import 'server-only';
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import {all} from '@erpx/server-runtime/public/runtime/sql';
import { ApiError } from "@erpx/server-runtime/public/http";
import { loadPms } from "../../../../packages/server-pms/src/public/pms";
import { pmsAccountCategory, pmsAccountCode } from "@erpx/server-finance/public/hospitality-accounting";
import { createHospitalitySetup, type PmsSetupRepository } from "@erpx/server-pms/application/hospitality-setup";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { pmsHospitalitySetupOrganizationFacts } from "@erpx/server-organization/public/pms-hospitality-setup-facts";
import { pmsHospitalitySetupCommerceFacts } from "@erpx/server-commerce/public/pms-hospitality-setup-facts";
import { pmsHospitalitySetupPosFacts } from "@erpx/server-pos/public/pms-hospitality-setup-facts";
import { pmsHospitalitySetupIdentityAccessFacts } from "@erpx/server-identity_access/public/pms-hospitality-setup-facts";
import { pmsHospitalitySetupFinanceFacts } from "@erpx/server-finance/public/pms-hospitality-setup-facts";
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
        }) => all(db, 'SELECT id,name,state,business_date FROM pms_properties WHERE company_id=? AND branch_id=? ORDER BY name', facts.company, facts.branch),
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
