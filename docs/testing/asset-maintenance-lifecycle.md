# Assets และ Maintenance — lifecycle จำลอง

## Flow ที่ทำงานแล้ว

- Asset register เก็บ Company/Branch/Department, code, acquisition snapshot, currency/cost, warranty, condition และ lifecycle version โดยแก้ source identity/cost ย้อนหลังไม่ได้
- Placement เปลี่ยน asset เป็น in service พร้อมสถานที่และ custodian ที่ยังเป็นพนักงาน active; condition inspection เก็บ good/degraded/unsafe เป็น immutable event
- Maintenance work order ผูก asset/version, owner ที่ยังมี assignment/HR engagement/สิทธิ์ใน scope, priority และ schedule; งานเข้าคิว Work/Inbox ของ owner
- Start เปลี่ยน asset เป็น maintenance และเริ่ม downtime ใน transaction เดียว; block ต้องมีเหตุผล, resume เก็บ event และ completion ต้องมี evidence กับ resulting condition
- Complete/Cancel ปิด downtime และคืน asset เข้าบริการพร้อม condition ใหม่; unsafe ยังคง maintenance เพื่อไม่ปล่อยของไม่ปลอดภัยกลับใช้งาน
- Retirement request ถูกปฏิเสธเมื่อมี work order ค้าง และต้องให้ reviewer ที่ยัง eligible และเป็นคนละคนกับผู้ขออนุมัติ ก่อนล้าง placement/custody

## หลักฐานจำลอง

Owner tests 4 เคสและ migration baseline รวม 5 tests ผ่าน ครอบคลุม full lifecycle, downtime/block/resume/completion, active-work retirement guard, revoked-owner recovery, stale concurrency, scope และ immutable histories ฐาน migration เพิ่ม 4 ตาราง รวม 372 ตาราง

## ขอบเขตที่ยังเหลือ

ยังไม่มี depreciation/Finance posting, serial/attachment evidence, parts reservation/cost, recurring preventive schedules, Calendar, browser หลายบทบาท และ business/UAT acceptance จึงนับ 10 catalog capabilities เป็น Implemented เท่านั้น ไม่ใช่ S10/S11 หรือ production acceptance
