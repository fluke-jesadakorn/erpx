# Field Service — lifecycle จำลอง

> Recorded report — findings, counts, gaps and hostname observations below belong to the original run. Current workflows: [owner catalogue](../contracts/next/catalogue.md); latest recorded engineering/provider/release boundaries: [verification inventory](../status/capability-delivery-matrix.md). This report is retained evidence, not an active plan.

## Flow ที่ทำงานแล้ว

- Service agreement ผูก customer, branch/department, covered asset, ช่วงวันที่, currency และ visit rate เป็น versioned source; ผู้สร้างอนุมัติเองไม่ได้และ reviewer ต้องยังมีสิทธิ์จริงใน scope
- Service order เริ่มจาก Support ticket/version ของ customer เดียวกับ agreement ที่ active และยังอยู่ในช่วงคุ้มครอง พร้อมตรวจ asset, technician, Company/Branch/Department และเก็บ source snapshot ที่แก้ย้อนหลังไม่ได้
- Schedule/reassign ตรวจ current assignment, principal และ HR engagement; คนหมดสิทธิ์ปรากฏเป็น routing blocker ใน Work และ manager ที่ eligible แก้ผู้รับได้
- การจองอะไหล่ใช้ `inventory_active_reservations` ร่วมกับ Sales/POS ตรวจ available stock และ quality hold ใน transaction เดียว; concurrent last-unit reservation สำเร็จเพียงรายการเดียว
- Dispatch ปฏิเสธ reservation ที่หมดอายุ; visit รองรับ start, site exception, resume, issue part และ completion ที่ต้องมี labor, evidence และ customer confirmation
- การ issue ลดทั้งจำนวนและมูลค่าสินค้าตามสัดส่วน หลัง consume reservation ใน transaction เดียว เพื่อให้ stock guard เห็นยอดจองที่ถูกใช้แล้ว
- Billing release ทำได้หลัง completion โดย reviewer ที่ยัง eligible และต้องเป็นคนละคนกับ technician; เก็บ customer, agreement, currency, amount และ source order version เป็น immutable obligation พร้อมสร้าง draft receivable Finance invoice ที่ผูก `source_type=field_service` และ `source_id` ของ service order ใน transaction เดียว โดยต้องมี receivable control account และ revenue account ที่ active ในสกุลเงินเดียวกัน. Finance approver คนละคนสามารถ post invoice ต่อด้วย invoice workflow ปกติ และ invoice ยังคง source linkage เดิม
- Agreement กำหนด `warrantyDiscountBps` แบบ versioned และแก้ย้อนหลังไม่ได้. เมื่อ service order ผูก asset ที่ยังไม่พ้น `warranty_end` ระบบคำนวณส่วนลดนั้นใน transaction เดียว, บันทึกว่าใช้ warranty, อัตรา, วันคำนวณ และ `warranty_end` snapshot ใน billing release/invoice; ถ้าหมด warranty จะคิด visit rate เต็มและไม่อนุญาตให้ client ส่งราคามาเอง

## หลักฐานจำลอง

Owner integration tests 5 เคสครอบคลุม agreement review, Support handoff, Work visibility, dispatch/visit exception, shared stock, customer sign-off, independent billing release, warranty pricing, last-unit concurrency, scope, stale version, revoked-technician recovery และ immutable history. Migration เพิ่ม warranty policy/evidence columns และทำให้ baseline รวม 378 ตาราง

## ขอบเขตที่ยังเหลือ

Finance invoice ที่สร้างจาก release ยังเป็น draft และต้องผ่าน Finance review/post/collection ตาม workflow ปกติ; travel/labor costing, attachment bytes, customer Portal, Calendar, multi-role browser และ business/UAT acceptance ยังไม่ผ่าน จึงนับ catalog capabilities เป็น Implemented เท่านั้น ไม่ใช่ S10 หรือ production acceptance
