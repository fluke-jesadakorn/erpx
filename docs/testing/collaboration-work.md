# Collaboration — files, typed relations และ action cards

> Recorded report — findings, counts, gaps and hostname observations below belong to the original run. Current workflows: [owner catalogue](../contracts/next/catalogue.md); latest recorded engineering/provider/release boundaries: [verification inventory](../status/capability-delivery-matrix.md). This report is retained evidence, not an active plan.

## พฤติกรรมที่ทำงานแล้ว

- Conversation files ใช้ private draft upload, ตรวจ media/size, bind ได้ครั้งเดียวกับ message และดาวน์โหลดได้เฉพาะสมาชิก room ที่ยังมีสิทธิ์; การถอนสมาชิกทำให้ไฟล์เดิมอ่านไม่ได้ทันที
- Typed relation รองรับ Commerce order, Support ticket, Field Service order และ staff task โดยอ่าน source ผ่าน owner service ก่อนสร้าง เก็บ version snapshot ของสองฝั่ง มี symmetric duplicate guard สำหรับ related/duplicates และแสดง backlink เฉพาะเมื่อผู้ดูยังอ่านทั้งสอง source ได้
- Relation removal เก็บผู้ทำ เหตุผล เวลา และ immutable event; source/target/type/version เดิมแก้ทับไม่ได้
- Action card ผูก source/version, owner, due time และ Work/Inbox; owner ต้องยังมี assignment, principal, HR engagement และ records.write ใน scope ปัจจุบัน
- Completion ต้องทำโดย owner พร้อม evidence; reviewer จึง reassign/cancel ได้ และ owner ที่หมดสิทธิ์แสดง routing blocker โดยไม่เปลี่ยน source record

## หลักฐานจำลอง

Related regression 34 tests / 4 files ผ่าน รวม owner tests 3 เคสและ Messaging attachments ครอบคลุม symmetric duplicate, removal, immutable histories, current recipient, evidence completion, revoked-owner recovery, membership-revoked download และ migration baseline 382 ตาราง

## ขอบเขตที่ยังเหลือ

Source adapters มีสี่ domain, ยังไม่มี relation graph search, retention/export/destruction policy, Legal hold สำหรับ conversation files, Calendar และ multi-role browser/business acceptance จึงนับสาม catalog capabilities เป็น Implemented ไม่ใช่ S12/S20 หรือ production acceptance
