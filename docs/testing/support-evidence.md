# Support — ไฟล์หลักฐานส่วนตัว

> Recorded report — findings, counts, gaps and hostname observations below belong to the original run. Current workflows: [owner catalogue](../contracts/next/catalogue.md); latest recorded engineering/provider/release boundaries: [verification inventory](../status/capability-delivery-matrix.md). This report is retained evidence, not an active plan.

ไฟล์ของ ticket ใช้ upload intent/private object store/attachment download เดิม เพิ่ม owner type `support_ticket` และ version ต้นทางเฉพาะเจ้าของนี้ ไม่เปลี่ยนความหมาย upload เดิม

## ขั้นตอนและสิทธิ์

- เจ้าของเคสที่ eligible หรือ Support manager แนบ JPG/PNG/WebP/PDF/CSV ไม่เกิน10MB ขณะ open/in_progress/waiting_customer ได้ แนบได้สูงสุด10ไฟล์ที่ยังใช้ พร้อมนับ intent ที่ยังไม่หมดอายุ
- เลือกไฟล์แล้วกด Attach selected file; หากเครือข่ายขาดให้ retry ไฟล์เดิม ใช้ intent/key เดิม ไม่สร้างไฟล์ซ้ำ เมื่อ version เคสเปลี่ยนต้องโหลดข้อมูลล่าสุดก่อนเลือกอัปโหลดใหม่
- ตรวจสิทธิ์/version ทั้งตอนจองและตอนรับไฟล์; ตรวจขนาด/signature/checksum และจัดเก็บ private ก่อน commit reference+version+immutable ticket event ใน transaction เดียว
- Download ตรวจ current source access ของ ticket ทุกครั้ง มี private/no-store และ attachment/nosniff ไม่เปิดไฟล์ภายใน inline ให้ลูกค้าโดยอัตโนมัติ
- Withdraw ต้องมีเหตุผล ใช้ version เคสปัจจุบัน เก็บ metadata/bytes และ event ไว้แต่ downloadเดิมอ่านไม่ได้ ไฟล์ legal hold ถอนออกไม่ได้
- หลังเสนอ resolution หลักฐาน frozen; หากต้องแก้ต้องให้ manager ส่งเคสกลับเพื่อ rework ตาม owner workflow ไม่แก้ผลที่เสนอแล้วทับ

## การทดสอบ

API integration ผ่าน existing reserve/PUT/download: replay, private bytes/checksum, Company/Branch/role denial, source version เปลี่ยนระหว่าง upload, ถอน assignment, legal hold, signatureผิด, frozenหลังresolution และจองครบ10แล้วยังrecoverresponseของintentเดิมได้

ร่วม regression ของ Support/incident/expense uploader และตรวจ browser บน disposable fixture ข้อมูลลูกค้าและไฟล์เป็น simulationเท่านั้น

## ขอบเขตที่ยังเหลือ

ยังไม่รองรับ incident attachments, customer Portal upload/share/revoke authority, retention/destruction workflow ของ Support หรือ multi-role customer acceptance ไม่เปิด external notification/provider gate จากผลนี้ Source evidenceไม่ใช่ลายเซ็นหรือการยอมรับจากลูกค้าจริง
