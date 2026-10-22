export function pmsAttachmentPurchase(targetType: string, purposeKey: string) {
  const capability = targetType === "pms_guest" || targetType === "pms_reservation" ? "pms.reservations"
    : targetType === "pms_stay" ? "pms.front-desk"
      : targetType === "pms_task" || targetType === "pms_lost_property" ? "pms.housekeeping"
        : targetType === "pms_room_type" || targetType === "pms_property" && purposeKey === "booking_media" ? "pms.rooms"
          : targetType === "pms_property" && purposeKey === "ota_reconciliation" ? "pms.guest-accounts" : null;
  return capability ? { capability, purpose: purposeKey === "booking_media" ? "new-work" as const : "settle-existing" as const } : null;
}
