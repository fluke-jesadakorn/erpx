import type { PeopleLifecycleData } from "@/lib/people-lifecycle";

type PositionGpo = PeopleLifecycleData["positions"][number]["gpo"];
type PersonGpoDelivery = PeopleLifecycleData["roster"][number]["gpoDelivery"];

export function positionGpoStatus(gpo: PositionGpo): string {
  if (!gpo.mapped) return "GPO: no mapping yet";
  if (gpo.directoryStatus === "blocked") return "GPO: needs IT attention";
  if (gpo.directoryStatus === "directory_applied")
    return "GPO: Directory configuration verified";
  return "GPO: Directory configuration pending";
}

export function personGpoStatus(delivery: PersonGpoDelivery): string {
  switch (delivery) {
    case "awaiting_identity":
      return "GPO delivery: waiting for a Directory account";
    case "queued":
      return "GPO delivery: in progress";
    case "directory_applied":
      return "GPO delivery: Directory change verified";
    case "blocked":
      return "GPO delivery: needs IT attention";
    case "none":
      return "GPO delivery: no mapping to apply";
  }
}
