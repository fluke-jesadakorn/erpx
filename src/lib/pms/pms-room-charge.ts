import { z } from "zod";
import { idField, moneyField, records, type Snapshot } from "@/server/modules/pms/public/rules/pms";
import { need } from "@/lib/pms/pms-engine";

/** Validates the POS-owned source before PMS posts the guest account charge in the same transaction. */
export const roomChargeSchema = z.strictObject({
  source: z.literal("pos"),
  sourceId: idField,
  sourceLineId: idField,
  companyId: idField,
  branchId: idField,
  propertyId: idField,
  stayId: idField,
  currency: z.string().length(3),
  revenueOwner: z.literal("pms"),
  sourceJournalId: z.null(),
  description: z.string().trim().min(1).max(2000),
  net: moneyField,
  service: moneyField,
  tax: moneyField,
  total: moneyField.positive(),
});

export function prepareRoomCharge(snapshot: Snapshot, input: unknown) {
  const charge = roomChargeSchema.parse(input),
    property = snapshot.property;
  need(
    charge.companyId === property.companyId &&
      charge.branchId === property.branchId &&
      charge.propertyId === property.id,
    "PMS_CHARGE_SCOPE",
    "The service item must belong to the same company, branch and hotel.",
  );
  need(
    charge.currency === property.config.currency,
    "PMS_CHARGE_CURRENCY",
    "The currency must match the hotel.",
  );
  need(
    charge.net + charge.service + charge.tax === charge.total,
    "PMS_CHARGE_TOTAL",
    "The service total does not match its components.",
  );
  const sourceKey = `pos:${charge.sourceId}:${charge.sourceLineId}`;
  const existing = records(snapshot, "entry").find(
    (e) => e.data.sourceKey === sourceKey,
  );
  if (existing) {
    need(
      existing.data.stayId === charge.stayId &&
        existing.data.net === charge.net &&
        existing.data.service === charge.service &&
        existing.data.tax === charge.tax &&
        existing.data.description === charge.description,
      "PMS_CHARGE_CONFLICT",
      "The existing service entry has different details.",
    );
    return { duplicate: true as const, entryId: existing.id, sourceKey };
  }
  const stay = records(snapshot, "stay").find((s) => s.id === charge.stayId);
  need(
    property.state === "open" && stay?.data.state === "in_house",
    "PMS_CHARGE_STAY",
      "Services are available only for checked-in guests on an open business day.",
  );
  const guestAccount = records(snapshot, "guest_account").find(
    (f) => f.id === stay.data.guestAccountId,
  );
  need(
    guestAccount && guestAccount.data.state !== "closed" && !guestAccount.data.invoiceId,
    "PMS_CHARGE_GUEST_ACCOUNT",
      "The guest account must be open and not yet invoiced.",
  );
  return { duplicate: false as const, guestAccountId: guestAccount.id, sourceKey, charge };
}
