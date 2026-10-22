export const pmsSocialEvents: Record<string, {
    label: string;
    group: string;
}> = {
    "pms.reservation.create": { label: "ยืนยันการจอง / Reservation confirmed", group: "reservation" },
    "pms.reservation.cancel": { label: "ยกเลิกการจอง / Reservation cancelled", group: "reservation" },
    "pms.stay.check_in": { label: "เช็กอิน / Check-in", group: "stay" },
    "pms.stay.check_out": { label: "เช็กเอาต์ / Check-out", group: "stay" },
    "pms.task.complete": { label: "ทำความสะอาดเสร็จ / Room prepared", group: "room" },
    "pms.task.inspect": { label: "ตรวจห้องแล้ว / Room inspected", group: "room" },
    "pms.payment.request_transfer": { label: "เงินโอนรอตรวจ / Bank transfer awaiting Finance", group: "transfer" },
    "pms.payment.verify_transfer": { label: "Finance ยืนยันเงินโอน / Bank transfer verified", group: "transfer" },
    "pms.payment.cancel_transfer": { label: "ยกเลิกการตรวจเงินโอน / Transfer request cancelled", group: "transfer" },
    "pms.charge.correction.request": { label: "ขอตรวจแก้ค่าบริการ / Charge correction requested", group: "correction" },
    "pms.charge.correction.review": { label: "ตรวจคำขอแก้ค่าบริการ / Charge correction reviewed", group: "correction" },
    "pms.refund.request": { label: "ขอคืนเงิน / Refund requested", group: "refund" },
    "pms.refund.approve": { label: "อนุมัติคืนเงิน / Refund approved", group: "refund" },
    "pms.refund.reject": { label: "ปฏิเสธคืนเงิน / Refund rejected", group: "refund" },
    "pms.refund.pay": { label: "จ่ายเงินคืนแล้ว / Refund paid", group: "refund" },
    "pms.shift.close": { label: "ปิดกะแคชเชียร์ / Cashier shift closed", group: "shift" },
    "pms.shift.review": { label: "ตรวจส่วนต่างกะแล้ว / Cash variance reviewed", group: "shift" },
    "pms.night.step": { label: "ผลปิดวัน / Night Audit progress", group: "night" },
};
