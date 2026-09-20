# Support ticket: Sales → Support → customer confirmation review

ชุด D / S04,S10,S15: owner implementation สำหรับ ticket ที่ผูก quotation/sales order. เพิ่ม migration0216 (350 tables), support_agent/support_manager และสิทธิ์ support.read/create/work/manage; Sales อ่าน/เปิดเคสได้ แต่ทำงานแทนเจ้าของไม่ได้. API/UI/Work/Inbox/Calendar ใช้สถานะต้นทางเดียวกัน. ยังไม่ใช่ Support ทั้ง capability หรือ production sign-off.

## การใช้งาน

1. Sales เลือกเอกสารลูกค้า ระบุปัญหา ผู้รับที่ eligible และวันติดตาม. Staff ที่มี Support create ใช้ source picker แบบจำกัดข้อมูลได้โดยไม่เห็นราคา/ต้นทุนหรือสิทธิ์แก้ Sales.
2. Support agent รับจาก Work/Inbox → Accept case → ทำงาน. ถ้ารอข้อมูลใช้ Request customer information พร้อมเหตุผล แล้ว Resume เมื่อได้คำตอบ. การบันทึกนี้ไม่ส่งข้อความออกภายนอก.
3. Record proposed resolution เปลี่ยนเป็น resolved และแจ้ง independent Support managers ใน scope. ยังไม่ถือว่าลูกค้ายอมรับ/เคสปิด. ผู้แก้เคสอนุมัติผลตัวเองไม่ได้ แม้เปลี่ยนผู้รับหลังแก้เสร็จ.
4. Sales บันทึก customer contact note จากเอกสารต้นทางหลัง resolution. Manager ตรวจ body/time/version ของ note ที่ตรงลูกค้า/เอกสาร แล้วบันทึกเหตุผลยืนยันผลแบบ staff-assisted. หลักฐานก่อน resolution, คนละ source, versionเก่า หรือ archived ใช้ไม่ได้. นี่ไม่ใช่การยืนยันตรงจาก Portal.
5. Manager คืนงานจาก resolved ให้แก้ใหม่ หรือ reopen closed case เมื่อมี dispute ได้; resolution เดิมคงอยู่ใน immutable events. Cancel ต้องมีเหตุผล. Reassignment ต้องเลือกคนที่มีสิทธิ์และ engagement ปัจจุบัน.

## ขอบเขตและการป้องกัน

Typed commands, expectedVersion, idempotency receipts, current assignment/employee eligibility, Company/Branch/Department scope, immutable event snapshots. การเปลี่ยน customer/scope ของ source order หลังมี ticket ถูกกั้น. ไม่กระทบสินค้า เงิน invoice หรือ GL. Work แสดง blocked reason ถ้าผู้รับหมดสิทธิ์หรือไม่มี independent manager; การแก้ routing ยังทำโดย manager/admin เดิม.

Customer 360 อ่านเฉพาะ Support title/action/version/source link เมื่อผู้ใช้มี support.read เพิ่มจาก commerce.read; ไม่ส่ง issue/reason/event payload เข้าภาพรวม. Support manager เห็น contact body ของ source เดียวกันเพื่อทบทวน confirmation โดยไม่ได้รับสิทธิ์แก้เอกสาร Sales.

## ยังต้องทำ

Standalone customer identity/authority, Portal replies/confirmation, public/internal reply separation, Email/outbox delivery, private attachments, incidents, scoped support sessions/capabilities, SLA policy snapshot/business calendars/pause/escalation และ routing repair queue กลาง. วันที่ติดตามปัจจุบันเป็นวันที่ผู้มอบงานกำหนด ไม่ใช่ SLA clock. Browser/UAT/provider gates บันทึกแยก; ไม่มีการประกาศครบ S04/S10/S15 จาก owner tests.
