import { records, type Snapshot } from "./pms-model";
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
      r.data.typeId === typeId &&
      ["reserved", "in_house"].includes(r.data.state) &&
      r.data.arrival <= date &&
      r.data.departure > date,
  ).length;
  const groups = records(s, "block")
    .filter(
      (r) =>
        r.id !== excludeBlock &&
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
            r.data.pickedStayIds.filter((id) => {
              const stay = records(s, "stay").find((x) => x.id === id);
              return (
                stay &&
                ["reserved", "in_house"].includes(stay.data.state) &&
                stay.data.arrival <= date &&
                stay.data.departure > date
              );
            }).length,
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
