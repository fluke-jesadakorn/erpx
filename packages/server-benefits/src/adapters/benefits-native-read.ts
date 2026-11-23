import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError } from "@erpx/server-runtime/public/http";
import {
  nativeBenefitsWorkspace,
  nativeBenefitDetail,
  type NativeBenefitDetail,
  type NativeBenefitsWorkspace,
} from "@erpx/feature-benefits/public/native-benefits-workspace-page";
import { benefitWorkItems, type BenefitRow } from "@erpx/server-benefits/domain/benefits";
import type { WorkItem } from "@erpx/feature-work_projection/lib/work-item";

/** Dependents always belong to the actor's Person; elections are the fallback
 * only for the personal scope, where every returned election is their own. */
function actorPerson(data: NativeBenefitsWorkspace): BenefitRow | null {
  const id = data.dependents[0]?.person_id ?? (!data.canReview ? data.elections[0]?.person_id : undefined);
  return id ? { id } : null;
}

function redact(items: WorkItem[], purchase: NativeBenefitsWorkspace["purchase"]): WorkItem[] {
  if (!purchase || purchase.settleExisting) return items;
  return items.map(item => ({
    ...item,
    canAct: false,
    queue: "waiting" as const,
    relationships: item.relationships?.filter(value => value !== "approval"),
  }));
}

/** Native enrollment workspace; the mounted client is the sole authority. */
export function benefitsWorkspace(a: WorkContext, personal = false): Promise<NativeBenefitsWorkspace> {
  return nativeBenefitsWorkspace(a, personal);
}

/** Native election detail; the mounted client is the sole authority. */
export function benefitDetail(a: WorkContext, id: string): Promise<NativeBenefitDetail> {
  return nativeBenefitDetail(a, id);
}

/** Native work queue. Missing scope entitlement is an empty queue, not an error. */
export async function benefitWork(a: WorkContext): Promise<WorkItem[]> {
  let data: NativeBenefitsWorkspace;
  try {
    data = await nativeBenefitsWorkspace(a);
  } catch (error) {
    if (error instanceof ApiError && (error.status === 403 || error.status === 404)) return [];
    throw error;
  }
  const active: BenefitRow[] = data.elections
    .filter(election => election.status !== "released" && election.status !== "cancelled")
    .map(election => ({ ...election }));
  return redact(
    benefitWorkItems(active, actorPerson(data), a.userId, data.canReview),
    data.purchase,
  );
}
