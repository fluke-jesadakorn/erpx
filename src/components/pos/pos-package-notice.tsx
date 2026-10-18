"use client";
import { InlineNotice } from "@/components/ui/foundation";
import { useFrontLanguage } from "@/components/front/front-language";
import type { PosWorkspace } from "@/server/modules/pos/public/pos";

export function PosPackageNotice({ data }: { data: PosWorkspace }) {
  const th = useFrontLanguage() === "th", access = data.purchases?.["pos.sales"];
  if (!access || access.newWork) return null;
  return <InlineNotice tone="warning">{access.settleExisting
    ? th ? "แพ็กเกจนี้เริ่มขายใหม่ไม่ได้ ยังรับชำระ คืนเงิน และปิดงานเดิมได้ตามสิทธิ์" : "New sales are unavailable. You can finish existing payments, refunds and closing with your current authority."
    : th ? "ดูประวัติที่บันทึกไว้ได้ แพ็กเกจปัจจุบันไม่รองรับการทำรายการ" : "Saved history is available. The current package does not permit transactions."}</InlineNotice>;
}
