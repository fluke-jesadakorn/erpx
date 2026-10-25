import type { WorkItem } from "@/lib/work-item";
import type { WorkContext } from "../../identity_access/public/authorization";
import { decodePmsSource, pmsSourceLink, type PmsSource } from "./source";
export type PmsHomeView = "mine" | "team" | "handover" | "company";
export type PmsHomeTask = WorkItem & {
    sourceKind: "pms_record";
    sourceVersion: number;
    propertyId: string;
    propertyName: string;
    businessDate: string;
    section: string;
    urgent: boolean;
    label: string;
};
export type TaskRow = PmsSource & {
    page_key: string;
    label: string;
    urgent: boolean;
    actionable: boolean;
    room_number: string | null;
};
export type PmsHotelOverview = {
    id: string;
    name: string;
    branchId: string;
    businessDate: string;
    state: string;
    capacity: number;
    sold: number;
    inspected: number;
    roomCount: number;
    arrivals: number;
    departures: number;
    currency: string;
    occupancy: number | null;
    roomRevenue: number | null;
    adr: number | null;
    revpar: number | null;
    receivable: number | null;
    advance: number | null;
    deposit: number | null;
};
const labels: Record<string, string> = { arrivals: "Check in", departures: "Check out", housekeeping: "Prepare room", inspection: "Inspect room",
    transfers: "Verify bank transfer", corrections: "Review charge correction", refunds: "Review refund", variance: "Review cash variance", shift: "Close cashier shift", night: "Resolve day-close blockers" };
export function taskView(row: TaskRow, actor: WorkContext): PmsHomeTask {
    row = decodePmsSource(row);
    const link = pmsSourceLink(row);
    const query = new URLSearchParams(link.href.split("?")[1]);
    const payout = row.kind === "refund" && row.data.state === "approved";
    const action = payout ? "Pay approved refund" : labels[row.label];
    const approval = ["transfer", "correction", "refund"].includes(row.kind) && row.actionable && !payout;
    if (!approval)
        query.delete("queue");
    if (row.kind === "stay")
        query.set("queue", row.label);
    const href = row.kind === "transfer" ? `/finance/hotels?property=${row.property_id}&transfer=${row.id}` : `/pms/${link.section}?${query}`;
    const room = row.room_number ?? (row.kind === "room" ? row.data.number : undefined);
    const assigned = "assigneeId" in row.data && row.data.assigneeId === actor.userId;
    return { id: row.id, type: "pms", sourceKind: "pms_record", sourceVersion: Number(row.version), propertyId: row.property_id,
        source: { kind: row.kind, id: row.id, version: Number(row.version), href, label: link.label }, actionKind: approval ? "approval" : "work",
        propertyName: row.property_name, businessDate: row.business_date, section: link.section, label: row.label, urgent: row.urgent,
        title: `${row.property_name}${room ? " · " + room : ""} · ${action}`,
        detail: `Business date ${row.business_date}`, href, status: row.data.state ?? ("cleanliness" in row.data ? String(row.data.cleanliness) : "pending"),
        updatedAt: Number(row.updated_at), queue: row.actionable ? "todo" : "waiting", canAct: row.actionable,
        responsibleQueue: row.kind === "transfer" ? "Finance approver" : payout ? "Cashier" : undefined,
        nextAction: row.actionable ? (row.kind === "transfer" ? "Match imported bank evidence to this hotel transfer" : action) : "Waiting for independent review", blocker: row.urgent ? labels[row.label] : null,
        relationships: [approval ? "approval" : assigned || payout ? "assigned" : "team"] };
}
