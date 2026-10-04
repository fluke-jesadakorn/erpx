# Customer contact notes — simulation implementation

> Recorded report — findings, counts, gaps and hostname observations below belong to the original run. Current workflows: [owner catalogue](../contracts/next/catalogue.md); latest recorded engineering/provider/release boundaries: [verification inventory](../status/capability-delivery-matrix.md). This report is retained evidence, not an active plan.

บันทึกการติดต่อภายในจาก Sales order/quotation ที่มีอยู่: เปิดเอกสาร → Customer contact notes → ระบุช่องทาง เวลา และสรุปการติดต่อ. ไม่มีการส่ง Email หรือเผยแพร่ Portal จากการบันทึกนี้ และไม่เปลี่ยนราคา/กำหนดส่ง/ข้อตกลงบนเอกสารธุรกิจ.

ผู้มี `commerce.read` อ่านได้เฉพาะ source order ใน Company/Branch/Department ที่มีสิทธิ์; ผู้มี `commerce.sales` เพิ่มได้. ผู้บันทึกเดิมหรือ Sales manager ที่มี `commerce.approve` แก้ไข/เก็บเข้า archive พร้อมเหตุผล. ทุก version มี body และ actor ใน history ที่แก้ทับหรือลบไม่ได้. Archived note ยังอ่านประวัติได้และแก้ทับไม่ได้.

เมื่อมี contact evidence แล้ว ฐานข้อมูลห้ามย้าย source order ไปยังลูกค้าหรือ scope อื่น เพื่อไม่ให้ประวัติรั่วไปผิดลูกค้า. สำหรับ order ที่เลือกลูกค้าผิด ให้เก็บเหตุผลใน note/archive และสร้างเอกสารใหม่ตาม workflow; ไม่ย้ายประวัติโดยเงียบ.

ทดสอบบนฐานแยก: create/replay, correction/archive, source version, future timestamps, Company/Branch/Department, read permission, revoked assignment, competing corrections, immutable history, author/manager authority และการเปลี่ยน source customer. ชุด customer notes + commands + migration ผ่าน 9 tests / schema342tables.

ขอบเขตที่ยังไม่ครบ: notes ที่ไม่มี order, การรวมลูกค้า/consent, Portal authority, CRM/Support timeline และ browser หลายบทบาท. ไม่มี handoff ใหม่จาก note; คำรับปากลูกค้าต้องยืนยันผ่าน source order/contract. สถานะ Implemented ไม่ใช่รับรอง capability ทั้งหมดหรือเปิดใช้งานจริง.
