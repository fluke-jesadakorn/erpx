# CRM activities — customer follow-up simulation

> Recorded report — findings, counts, gaps and hostname observations below belong to the original run. Current workflows: [owner catalogue](../contracts/next/catalogue.md); latest recorded engineering/provider/release boundaries: [verification inventory](../status/capability-delivery-matrix.md). This report is retained evidence, not an active plan.

เปิด sales order/quotation → Assign customer follow-up → เลือกผู้รับที่มีสิทธิ์ Sales และยังทำงานอยู่ → กำหนดวันและ next action. ผู้รับเห็นงานใน Work, Inbox และ Calendar ของตน เปิด source order ได้ตามสิทธิ์เดิม และบันทึกผลการติดต่อเพื่อจบงาน.

เฉพาะผู้รับปัจจุบันปิดงานว่า complete ได้. Sales manager เปลี่ยนผู้รับ/วันนัดหรือยกเลิกพร้อมเหตุผลได้ แต่ไม่ปิดแทนว่าติดต่อสำเร็จ. ผู้รับหมดสิทธิ์หรือสิ้นสุด engagement ทำให้ completion ถูกปฏิเสธและ Work แสดง blocker. ไม่มีการเลือกคนแทนเงียบ ๆ. ทุกการเปลี่ยนเก็บ owner/date/reason/actor/version ในประวัติ immutable.

บันทึกผลไม่ได้ส่ง email/โทรออก ไม่เปลี่ยนข้อตกลงกับลูกค้า และไม่ได้หมายถึงลูกค้ารับมอบงาน. Source order ต้องอยู่ใน Company/Branch/Department ของผู้ใช้; รายการ คิว และรายละเอียดใช้ scope เดียวกัน. Source customer/scope เปลี่ยนไม่ได้เมื่อมี follow-up evidence.

ขอบเขต: follow-up จาก order/quotation เท่านั้น. ยังไม่มี CRM lead/opportunity, provider delivery, SLA business-calendar/escalation หรือ customer confirmation. Calendar ใช้วันที่แบบ all-day; ไม่อ้างชั่วโมง SLA. Browser และ UAT แยกจาก automated acceptance.
