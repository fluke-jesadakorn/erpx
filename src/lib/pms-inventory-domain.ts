import { records, type Snapshot, type Stay } from "./pms-model";
/** Legacy stays have one room type for their entire stay. */
export function stayTypeOnDate(stay: Stay, date: string) {
  return stay.roomTypePeriods?.find(period => period.from <= date && period.to > date)?.typeId ?? stay.typeId;
}
export function groupPickupCount(s: Snapshot, blockId: string, date: string, excludeStay = "") {
  const block = records(s, "block").find(row => row.id === blockId);
  if (!block) return 0;
  const picked = new Set(block.data.pickedStayIds);
  const live = records(s, "stay").filter(row => row.id !== excludeStay && picked.has(row.id) &&
    ["reserved", "in_house"].includes(row.data.state));
  const used = new Set(live.filter(row => row.data.arrival <= date && row.data.departure > date &&
    stayTypeOnDate(row.data, date) === block.data.typeId).map(row => row.id));
  const liveIds = new Set(live.map(row => row.id));
  for (const hold of records(s, "block")) {
    if (hold.data.state !== "active" || !hold.data.bookingChangeId || hold.data.from > date || hold.data.to <= date ||
        hold.data.expiresAt && hold.data.expiresAt <= (s.asOf ?? 0)) continue;
    for (const claim of hold.data.groupPickupClaims ?? [])
      if (claim.blockId === blockId && liveIds.has(claim.stayId)) used.add(claim.stayId);
  }
  return used.size;
}
/** Credit only this stay's unused allotment, on nights still held by its group. */
export function stayGroupBlock(s: Snapshot, stayId: string, typeId: string, date: string) {
  return records(s, "block").find(row => row.data.kind === "group" && row.data.state === "active" &&
    row.data.pickedStayIds.includes(stayId) && row.data.typeId === typeId && row.data.from <= date &&
    row.data.to > date && row.data.releaseDate > s.property.businessDate &&
    (!row.data.expiresAt || row.data.expiresAt > (s.asOf ?? 0)));
}
export function stayGroupAllowance(s: Snapshot, stayId: string, typeId: string, date: string) {
  const block = stayGroupBlock(s, stayId, typeId, date);
  return block ? Math.max(0, block.data.quantity - groupPickupCount(s, block.id, date, stayId)) : null;
}
export function availability(
  s: Snapshot,
  typeId: string,
  date: string,
  excludeStay = "",
  excludeBlock = "",
) {
  const rooms = records(s, "room").filter(
    (r) => r.data.typeId === typeId && r.data.active,
  );
  const unavailable = new Set(
    records(s, "block")
      .filter(
        (b) =>
          b.data.kind === "maintenance" &&
          b.data.state === "active" &&
          b.data.from <= date,
      )
      .map((b) => b.data.roomId),
  );
  const capacity = rooms.filter((r) => !unavailable.has(r.id)).length;
  const occupied = records(s, "stay").filter(
    (r) =>
      r.id !== excludeStay &&
      stayTypeOnDate(r.data, date) === typeId &&
      ["reserved", "in_house"].includes(r.data.state) &&
      r.data.arrival <= date &&
      r.data.departure > date,
  ).length;
  const groups = records(s, "block")
    .filter(
      (r) =>
        r.id !== excludeBlock &&
        !r.data.quotaOnly &&
        r.data.kind === "group" &&
        r.data.state === "active" &&
        (!r.data.expiresAt || r.data.expiresAt > (s.asOf ?? 0)) &&
        r.data.typeId === typeId &&
        r.data.from <= date &&
        r.data.to > date &&
        r.data.releaseDate > s.property.businessDate,
    )
    .reduce(
      (n, r) =>
        n +
        Math.max(
          0,
          r.data.quantity -
            groupPickupCount(s, r.id, date, excludeStay),
        ),
      0,
    );
  return {
    capacity,
    occupied,
    held: groups,
    available: capacity - occupied - groups,
  };
}
