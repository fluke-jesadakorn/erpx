import { availability } from "@erpx/feature-pms/lib/pms-inventory-domain";
import { pmsAccountKeys, records, type PmsRecord, type Property, type Snapshot } from "@erpx/server-pms/public/rules/pms-model";

export type PmsSetupFacts = { hasRoomTypes: boolean; hasActiveRooms: boolean; hasActiveRates: boolean };

export function dailyRoomReport(s: Snapshot, date: string) {
  const closed = records(s, "night").find(
    (n) => n.data.date === date && n.data.state === "completed",
  )?.data.roomMetrics;
  const capacity =
    closed?.capacity ??
    records(s, "room_type").reduce(
      (n, t) => n + availability(s, t.id, date).capacity,
      0,
    );
  const sold =
    closed?.sold ??
    records(s, "stay").filter(
      (st) =>
        ["in_house", "checked_out"].includes(st.data.state) &&
        st.data.arrival <= date &&
        st.data.departure > date,
    ).length;
  const all = records(s, "entry"),
    byId = new Map(all.map((e) => [e.id, e]));
  function isRoomNight(entry: PmsRecord<"entry">) {
    const seen = new Set<string>();
    let current: PmsRecord<"entry"> | undefined = entry;
    while (current && !seen.has(current.id)) {
      seen.add(current.id);
      if (current.data.sourceKey.startsWith("night:")) return true;
      current = byId.get(current.data.originalId);
    }
    return false;
  }
  const roomRevenue = all
    .filter(
      (e) =>
        e.data.date === date &&
        ["charge", "adjustment"].includes(e.data.kind) &&
        isRoomNight(e),
    )
    .reduce((n, e) => n + e.data.net, 0);
  return { capacity, sold, roomRevenue };
}
export function readinessFromFacts(property: Property, facts: PmsSetupFacts) {
  const c = property.config;
  return [
    !facts.hasRoomTypes ? "Add a room type" : null,
    !facts.hasActiveRooms ? "Add rooms" : null,
    !facts.hasActiveRates ? "Add a rate plan" : null,
    !c.approvedBy ? "Have Finance approve the setup" : null,
    !c.chartId || pmsAccountKeys.some((k) => !c.accounts[k])
      ? "Complete the chart of accounts setup"
      : null,
    c.currency !== "THB" || c.minorUnit !== 2 ? "The pilot supports THB" : null,
  ].filter(Boolean) as string[];
}
export function readiness(s: Snapshot, facts?: PmsSetupFacts) {
  return readinessFromFacts(s.property, facts ?? {
    hasRoomTypes: records(s, "room_type").length > 0,
    hasActiveRooms: records(s, "room").some(r => r.data.active),
    hasActiveRates: records(s, "rate").some(r => r.data.active),
  });
}
export function nightBlockers(s: Snapshot) {
  const d = s.property.businessDate;
  return [
    ...readiness(s),
    ...records(s, "stay")
      .filter(
        (r) =>
          (r.data.state === "reserved" && r.data.arrival <= d) ||
          (r.data.state === "in_house" && r.data.departure <= d),
      )
      .map(
        (r) =>
          `Process ${r.id.slice(0, 8)}: ${r.data.state === "reserved" ? "Not checked in / No-show" : "Checkout due"}`,
      ),
    ...records(s, "shift")
      .filter(
        (r) =>
          r.data.state === "open" ||
          (r.data.state === "closed" && r.data.variance !== 0),
      )
      .map((r) => `Close/review shift variance for ${r.data.name}`),
  ];
}
