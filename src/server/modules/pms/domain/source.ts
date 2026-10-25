import { hasPermission } from "@/lib/permissions";
import type { StaffActor } from "../../operations/public/staff-policy";
import { sourceRecordLabel } from "@/lib/source-record";
export type PmsSource = {
    id: string;
    property_id: string;
    company_id: string;
    branch_id: string;
    property_name: string;
    business_date: string;
    kind: string;
    data: {
        state?: string;
        number?: string;
        type?: string;
        roomId?: string;
        guestAccountId?: string;
        requestedBy?: string;
        recordedBy?: string;
        userId?: string;
        completedBy?: string;
        assigneeId?: string;
        date?: string;
    };
    version: number;
    updated_at: number;
};
export function decodePmsSource<T extends PmsSource>(source: T): T {
    return { ...source, data: typeof source.data === "string" ? JSON.parse(source.data) : source.data };
}
export function pmsSourceAllowed(actor: StaffActor, source: PmsSource) {
    const can = (permission: string) => hasPermission(actor, permission);
    switch (source.kind) {
        case "room":
        case "task":
        case "block": return can("pms.housekeeping") || can("pms.inspect") || can("pms.maintenance");
        case "stay":
        case "reservation": return can("pms.frontdesk") || can("pms.reservations") || can("pms.cashier");
        case "guest_account": return can("pms.cashier") || can("pms.reports");
        case "refund":
        case "correction": return can("pms.approve") || (can("pms.cashier") && source.data.requestedBy === actor.userId);
        case "transfer": return can("finance.approve") || (can("pms.cashier") && source.data.recordedBy === actor.userId);
        case "shift": return can("pms.approve") || can("pms.audit") || (can("pms.cashier") && source.data.userId === actor.userId);
        case "night": return can("pms.audit") || can("pms.reports");
        default: return false;
    }
}
export function pmsSourceLink(source: PmsSource) {
    const section = source.kind === "stay" ? "front-desk" : source.kind === "reservation" ? "reservations"
        : ["room", "task", "block"].includes(source.kind) ? "housekeeping" : source.kind === "shift" ? "cashier"
            : source.kind === "night" ? "night-audit" : "guest-accounts";
    const query = new URLSearchParams({ property: source.property_id, record: source.id });
    if (source.data.guestAccountId)
        query.set("guestAccount", source.data.guestAccountId);
    if (source.kind === "guest_account")
        query.set("guestAccount", source.id);
    if (source.kind === "task")
        query.set("task", source.id);
    if (["transfer", "refund"].includes(source.kind))
        query.set("queue", "approval");
    if (source.kind === "correction")
        query.set("queue", "corrections");
    return { section, sourceKind: source.kind, version: source.version, label: sourceRecordLabel(source.kind),
        href: `/pms/${section}?${query}`, title: `${source.property_name} · ${sourceRecordLabel(source.kind)}${source.data.number ? " " + source.data.number : ""}` };
}
