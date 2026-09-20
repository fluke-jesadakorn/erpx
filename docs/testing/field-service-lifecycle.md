# Field Service — lifecycle จำลอง

## Flow ที่ทำงานแล้ว

- Service agreement ผูก customer, branch/department, covered asset, ช่วงวันที่, currency และ visit rate เป็น versioned source; ผู้สร้างอนุมัติเองไม่ได้และ reviewer ต้องยังมีสิทธิ์จริงใน scope
- Service order เริ่มจาก Support ticket/version ของ customer เดียวกับ agreement ที่ active และยังอยู่ในช่วงคุ้มครอง พร้อมตรวจ asset, technician, Company/Branch/Department และเก็บ source snapshot ที่แก้ย้อนหลังไม่ได้
- Schedule/reassign ตรวจ current assignment, principal และ HR engagement; คนหมดสิทธิ์ปรากฏเป็น routing blocker ใน Work และ manager ที่ eligible แก้ผู้รับได้
- การจองอะไหล่ใช้ `inventory_active_reservations` ร่วมกับ Sales/POS ตรวจ available stock และ quality hold ใน transaction เดียว; concurrent last-unit reservation สำเร็จเพียงรายการเดียว
- Dispatch ปฏิเสธ reservation ที่หมดอายุ; visit รองรับ start, site exception, resume, issue part และ completion ที่ต้องมี labor, evidence และ customer confirmation
- การ issue ลดทั้งจำนวนและมูลค่าสินค้าตามสัดส่วน หลัง consume reservation ใน transaction เดียว เพื่อให้ stock guard เห็นยอดจองที่ถูกใช้แล้ว
- Billing release ทำได้หลัง completion โดย reviewer ที่ยัง eligible และต้องเป็นคนละคนกับ technician; เก็บ customer, agreement, currency, amount และ source order version เป็น immutable obligation

## หลักฐานจำลอง

Owner integration tests 3 เคสครอบคลุม agreement review, Support handoff, Work visibility, dispatch/visit exception, shared stock, customer sign-off, independent billing release, last-unit concurrency, scope, stale version, revoked-technician recovery และ immutable history. Migration เพิ่ม 6 ตาราง ทำให้ baseline รวม 378 ตาราง

## ขอบเขตที่ยังเหลือ

Billing release รอบนี้เป็น source obligation และยังไม่สร้าง Finance invoice อัตโนมัติ; warranty/chargeable policy, travel/labor costing, attachment bytes, customer Portal, Calendar, multi-role browser และ business/UAT acceptance ยังไม่ผ่าน จึงนับ catalog capabilities เป็น Implemented เท่านั้น ไม่ใช่ S10 หรือ production acceptance
