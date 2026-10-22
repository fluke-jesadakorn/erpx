export function posAttachmentPurchase(targetType: string) {
  const capability = targetType === "pos_sale" ? "pos.sales" : targetType === "pos_refund" ? "pos.refunds" : targetType === "pos_shift" ? "pos.shifts" : null;
  return capability ? { capability, purpose: "settle-existing" as const } : null;
}
