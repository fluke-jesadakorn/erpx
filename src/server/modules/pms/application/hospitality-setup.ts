import { hasPermission } from "@/lib/identity_access/permissions";
import { readiness } from "@/lib/pms/pms-engine";
import { pmsAccountKeys, records as pmsRecords, type Kind, type Snapshot } from "../domain/pms-model";
import type { WorkContext } from "../../identity_access/public/authorization";
import type { Clock } from "../../shared/application/ports";
export type PmsSetupRow = Record<string, string | number | boolean | null>;
const str = (row: PmsSetupRow, key: string) => String(row[key] ?? "");
export type PmsSetupRepository = {
    branches(facts: {
        company: string;
        branchId: string | null;
        branchId2: string | null;
    }): Promise<PmsSetupRow[]>;
    properties(facts: {
        company: string;
        branch: string;
    }): Promise<PmsSetupRow[]>;
    commerce(facts: {
        company: string;
    }): Promise<PmsSetupRow | null>;
    settings(facts: {
        company: string;
    }): Promise<PmsSetupRow | null>;
    warehouses(facts: {
        company: string;
        branch: string;
    }): Promise<PmsSetupRow[]>;
    registers(facts: {
        company: string;
        branch: string;
    }): Promise<PmsSetupRow[]>;
    roles(facts: {
        company: string;
        now1: number;
        now2: number;
        branch: string;
    }): Promise<PmsSetupRow[]>;
    periods(facts: {
        company: string;
        date: string;
        date2: string;
    }): Promise<PmsSetupRow[]>;
    charts(facts: {
        company: string;
    }): Promise<PmsSetupRow[]>;
    accounts(facts: {
        company: string;
    }): Promise<PmsSetupRow[]>;
    products(facts: {
        company: string;
    }): Promise<PmsSetupRow[]>;
    tables(facts: {
        company: string;
        branch: string;
    }): Promise<PmsSetupRow[]>;
    stock(facts: {
        company: string;
        branch: string;
    }): Promise<PmsSetupRow | null>;
    companyName(facts: {
        company: string;
    }): Promise<PmsSetupRow | null>;
    hotel(actor: WorkContext, id: string): Promise<Snapshot>;
};
export function createHospitalitySetup(ports: {
    repository(actor: WorkContext): Promise<PmsSetupRepository>;
    clock: Clock;
    error(status: number, code: string, message: string): Error;
    accountCode: typeof import("../../finance/public/hospitality-accounting").pmsAccountCode;
    accountCategory: typeof import("../../finance/public/hospitality-accounting").pmsAccountCategory;
    environment(): string | undefined;
    roomChargePilot(): string | undefined;
}) {
    async function hospitalitySetup(actor: WorkContext, branchId?: string, propertyId?: string) {
        const can = (p: string) => hasPermission(actor, p);
        if (actor.beta || actor.departmentId || !['pms.configure', 'pos.configure', 'pms.accounting.prepare', 'pms.accounting.approve'].some(can))
            throw ports.error(403, 'PERMISSION_REQUIRED', 'Hotel, restaurant or Finance setup access is required.');
        const db = await ports.repository(actor), company = actor.companyId;
        const branches = await db.branches({ company: company, branchId: actor.branchId, branchId2: actor.branchId });
        const branch = actor.branchId ?? branchId ?? str(branches[0] ?? {}, 'id');
        if (branch && !branches.some(r => r.id === branch))
            throw ports.error(404, 'BRANCH_NOT_FOUND', 'Choose an accessible company branch.');
        const properties = await db.properties({ company: company, branch: branch });
        const property = propertyId ? properties.find(r => r.id === propertyId) : properties[0];
        if (propertyId && !property)
            throw ports.error(404, 'PMS_NOT_FOUND', 'Hotel not found in this branch.');
        const finance = can('pms.accounting.prepare') || can('pms.accounting.approve');
        const pms = can('pms.configure') || finance;
        const snapshot = pms && property ? await db.hotel(actor, String(property.id)) : null;
        const date = snapshot?.property.businessDate ?? new Intl.DateTimeFormat('en-CA', { timeZone: 'Asia/Bangkok', year: 'numeric', month: '2-digit', day: '2-digit' }).format(new Date(ports.clock.now()));
        const [commerce, settings, warehouses, registers, roles, periods, charts, accounts, products, tables, stock] = await Promise.all([
            db.commerce({ company: company }),
            db.settings({ company: company }),
            db.warehouses({ company: company, branch: branch }),
            db.registers({ company: company, branch: branch }),
            db.roles({ company: company, now1: ports.clock.now(), now2: ports.clock.now(), branch: branch }),
            db.periods({ company: company, date: date, date2: date }),
            finance ? db.charts({ company: company }) : Promise.resolve([]),
            finance ? db.accounts({ company: company }) : Promise.resolve([]),
            can('pos.configure') ? db.products({ company: company }) : Promise.resolve([]),
            can('pos.configure') ? db.tables({ company: company, branch: branch }) : Promise.resolve([]),
            db.stock({ company: company, branch: branch }),
        ]);
        const roleNeeds = [['Hotel administrator', ['pms_manager', 'holding_owner']], ['Front desk', ['pms_frontdesk', 'pms_manager', 'holding_owner']], ['Hotel cashier', ['pms_cashier', 'holding_owner']], ['Restaurant cashier', ['pos_cashier', 'pos_manager', 'holding_owner']], ['Kitchen', ['pos_kitchen', 'holding_owner']], ['Housekeeping', ['pms_housekeeping', 'holding_owner']], ['Room inspector', ['pms_supervisor', 'holding_owner']], ['Finance preparer', ['finance_preparer', 'holding_owner']], ['Finance approver', ['finance_approver', 'holding_owner']]] as const;
        const missingRoles = roleNeeds.filter(([, valid]) => !roles.some(r => (valid as readonly string[]).includes(String(r.role)))).map(([label]) => label);
        const preparers = roles.filter(r => ['finance_preparer', 'holding_owner'].includes(String(r.role)));
        const independent = roles.some(r => ['finance_approver', 'holding_owner'].includes(String(r.role)) && preparers.some(p => p.user_id !== r.user_id));
        const records = snapshot?.records ?? [], rows = <K extends Kind>(kind: K) => snapshot ? pmsRecords(snapshot, kind) : [];
        const hotelIssues = snapshot ? readiness(snapshot) : ['Create the hotel'];
        const steps = [
            { id: 'location', title: 'Choose location', owner: 'Hotel administrator', href: '/people/organization', blockers: branch ? [] : ['Add an active hotel branch'] },
            { id: 'finance', title: 'Prepare Finance and stock', owner: 'Finance preparer', href: '/finance/setup', blockers: [...(!commerce ? ['Configure Commerce accounts'] : commerce.currency !== 'THB' || commerce.chart_currency !== 'THB' || commerce.chart_status !== 'active' ? ['Finance must review a THB Commerce chart'] : []), ...(periods.length !== 1 ? ['Open exactly one accounting period for the business date'] : []), ...(!warehouses.length ? ['Add a kitchen warehouse'] : [])] },
            { id: 'hotel', title: 'Configure the hotel', owner: 'Hotel administrator', href: '/pms/settings', blockers: pms ? [...(!snapshot ? ['Create the hotel'] : []), ...(!rows('room_type').length ? ['Add room types'] : []), ...(!rows('room').some(r => r.data.active) ? ['Add active rooms'] : []), ...(rows('room_type').some(t => !rows('rate').some(r => r.data.typeId === t.id && r.data.active && r.data.from <= date && r.data.to >= date && r.data.price > 0)) ? ['Enter a current nightly price for each room type'] : []), ...(!snapshot?.property.config.cancellationPolicy ? ['Enter a cancellation policy'] : [])] : ['Hotel setup requires hotel access'] },
            { id: 'restaurant', title: 'Configure the restaurant', owner: 'Restaurant manager', href: '/pos?tab=settings', blockers: [...(!settings?.enabled ? ['Configure and enable POS'] : settings.currency !== 'THB' ? ['Align POS currency with the hotel'] : []), ...(!registers.length ? ['Add a restaurant register in this branch'] : []), ...(can('pos.configure') ? [...(!tables.length ? ['Add restaurant tables'] : []), ...(!products.some(r => r.kind && Number(r.price_minor) > 0) || products.some(r => r.kind && Number(r.price_minor) <= 0) ? ['Add a priced menu and recipes'] : [])] : ['Menu review requires restaurant setup access']), ...(Number(stock?.n ?? 0) === 0 ? ['Approve opening ingredient stock'] : [])] },
            { id: 'team', title: 'Prepare the team', owner: 'Access administrator', href: '/people/settings', blockers: [...missingRoles, ...(!independent ? ['Assign different people to Finance preparation and approval'] : [])] },
            { id: 'open', title: 'Review and open', owner: 'Finance approver / Hotel administrator', href: '/pms/settings', blockers: [...hotelIssues, ...(snapshot?.property.state !== 'open' ? ['Verify readiness and open the hotel'] : []), ...(rows('room').some(r => r.data.active && r.data.cleanliness !== 'inspected') ? ['Housekeeping must complete and independently inspect rooms'] : [])] },
        ];
        return { companyId: company, companyName: str((await db.companyName({ company: company })) ?? {}, 'display_name'), branchId: branch, branches, properties, propertyId: property?.id ?? null, date,
            permissions: { hotel: can('pms.configure'), restaurant: can('pos.configure'), productCreate: can('inventory.prepare'), financePrepare: can('pms.accounting.prepare') && can('finance.write'), financeApprove: can('pms.accounting.approve') },
            steps, ready: steps.every(s => !s.blockers.length), hotel: snapshot ? { property: { ...snapshot.property, config: { ...snapshot.property.config, ...(!finance ? { accounts: Object.fromEntries(pmsAccountKeys.map(k => [k, ''])) as typeof snapshot.property.config.accounts, chartId: '' } : {}) } }, records: records.filter(r => ['room', 'room_type', 'rate'].includes(r.kind)) } : null,
            posConfigured: Boolean(settings), posCurrency: settings?.currency ?? null, warehouses, registers, products, tables, charts, accounts,
            suggestions: finance ? pmsAccountKeys.map(key => ({ key, code: ports.accountCode(key), category: ports.accountCategory(key) })) : [],
            roomChargeRequirements: ['Same company and branch', 'Matching THB currency and business date', 'Checked-in stay with an open, uninvoiced guest account', 'All kitchen items delivered', 'Entire bill unpaid'],
            activationRestrictions: ports.environment() === 'production' ? ['Production release gates apply', ...(ports.roomChargePilot() !== 'true' ? ['Restaurant room charging requires pilot activation'] : [])] : ['Rehearsal readiness does not approve production activation'],
        };
    }
    return { hospitalitySetup };
}
