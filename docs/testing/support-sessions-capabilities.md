# Support — temporary sessions และ scoped capabilities

> Recorded report — findings, counts, gaps and hostname observations below belong to the original run. Current workflows: [owner catalogue](../contracts/next/catalogue.md); latest recorded engineering/provider/release boundaries: [verification inventory](../status/capability-delivery-matrix.md). This report is retained evidence, not an active plan.

Support session ผูกกับ ticket และพนักงานผู้รับผิดชอบโดยตรง พนักงานขอสิทธิ์ให้ตนเองเท่านั้น กำหนดอายุได้ไม่เกิน 8 ชั่วโมง และ Support manager ที่ยังมี assignment/engagement/สิทธิ์ใน scope เดียวกันต้องเป็นคนละคนกับผู้ขอและผู้รับสิทธิ์จึงอนุมัติได้

## Flow ที่ทำงานแล้ว

- ผู้รับผิดชอบ ticket ที่ยัง active ขอ session จากหน้าจอ Temporary support access โดยแนบ ticket version, เหตุผล และเวลาหมดอายุ
- คำขอเข้าคิว Work/Inbox ของ independent Support manager; stale version และการตัดสินพร้อมกันสำเร็จได้รายการเดียว
- การอนุมัติ session อย่างเดียวยังอ่าน customer context ไม่ได้ Manager ต้องออก capability ที่ระบุชัด ได้แก่ `customer_order_context` หรือ `ticket_evidence_metadata`
- `customer_order_context` เปิดเผยเฉพาะเลข/ประเภท/สถานะ/วันครบกำหนด/สกุลเงินของ order และสถานะ ticket ไม่ส่งชื่อลูกค้า note ต้นทุน payment หรือไฟล์
- `ticket_evidence_metadata` เพิ่มเฉพาะชื่อไฟล์ ชนิด ขนาด checksum และเวลา ไม่เปิด file bytes หรือ download authority
- ทุก read ตรวจ session status, expiry, ticket state, Company/Branch/Department, current IAM principal, assignment, employee/engagement และ active capability ใหม่เสมอ
- revoke capability, reissue และ revoke session ใช้ optimistic version; revoke session เพิกถอน grant ที่ยัง active ใน transaction เดียว ประวัติแก้หรือลบไม่ได้

## หลักฐานจำลอง

Owner tests 4 เคสและ regression ที่เกี่ยวข้องรวม 14 tests ผ่าน ครอบคลุม independent approval, capability enforcement, revoke/reissue/cascade, expiry, revoked assignment, stale race, scope และ immutable history ฐาน migration เพิ่ม 4 ตาราง รวม baseline 368 ตาราง

## ขอบเขตที่ยังเหลือ

ยังไม่มี customer-originated consent, remote-control session, capability ที่เขียนข้อมูล, file-byte grant, Calendar, retention/export, browser หลายบทบาท และ UAT ลูกค้าจริง จึงนับเป็น Implemented พร้อม automated observation เท่านั้น ไม่ใช่ scenario S10 หรือ production acceptance
