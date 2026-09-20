# Learning — lifecycle จำลอง

## Flow ที่ทำงานแล้ว

- Course catalog เก็บ title, provider, duration, content description และ optional eligibility code เป็น versioned draft; ผู้สร้าง publish เองไม่ได้และ reviewer ต้องมีสิทธิ์ปัจจุบัน
- Assignment เลือกได้เฉพาะ published course/version และ active worker ที่มี HR engagement จริง พร้อม snapshot Branch/Department, owner และ due date
- Assignment เข้า Work/Inbox ของ learner; ผู้จัดการทำ learner start/complete แทนไม่ได้
- Learner บันทึก attendance, score และผลหลักฐานก่อน completion; reviewer ใช้ waive/cancel เป็น exception ที่มีเหตุผลได้
- Completed หรือ waived course ที่มี eligibility code สร้าง immutable handoff ผูก person และ source assignment version เพื่อให้ Benefits/Operations ตรวจต่อโดยไม่เดาผลจากสถานะ
- Course/assignment sources และ event history แก้ย้อนหลังหรือลบไม่ได้; worker ที่หมดสิทธิ์แสดง blocker และไม่เริ่มงานต่อได้

## หลักฐานจำลอง

Related regression 56 tests / 4 files ผ่าน รวม owner tests 3 เคส ครอบคลุม independent publication, assignment, learner-only progress/result, evidence completion, reviewer waiver, eligibility handoff, revoked learner, source immutability, Work/Inbox และ migration baseline 387 ตาราง

## ขอบเขตที่ยังเหลือ

Provider import/acknowledgement, certificate file/expiry/renewal, classroom roster, Calendar, Benefits consumer workflow, multi-role browser และ business/UAT acceptance ยังไม่ผ่าน จึงนับ 5 catalog capabilities เป็น Implemented ไม่ใช่ S18 หรือ production acceptance
