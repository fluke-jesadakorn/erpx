"use client";
import { InlineNotice } from "@erpx/ui/components/foundation";
import { useFrontLanguage } from "@erpx/front/components/front-language";
import type { PmsWorkspace } from "../../../../packages/server-pms/src/public/pms";

export function PmsPackageNotice({ data }: { data: PmsWorkspace }) {
  const th = useFrontLanguage() === "th", access = data.purchases?.["pms.front-desk"];
  if (!access || access.newWork) return null;
  return <InlineNotice tone="warning">{access.settleExisting
    ? th ? "แพ็กเกจนี้เริ่มงานใหม่ไม่ได้ ยังดูแลการเข้าพัก รับชำระ คืนเงิน และปิดงานเดิมได้ตามสิทธิ์" : "New work is unavailable. You can finish existing stays, payments, refunds and closing with your current authority."
    : th ? "ดูประวัติโรงแรมที่บันทึกไว้ได้ แพ็กเกจปัจจุบันไม่รองรับการทำรายการ" : "Saved hotel history is available. The current package does not permit transactions."}</InlineNotice>;
}
