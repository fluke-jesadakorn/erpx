# Company routing review — ชุด A (บางส่วน)

> Recorded report — findings, counts, gaps and hostname observations below belong to the original run. Current workflows: [owner catalogue](../contracts/next/catalogue.md); latest recorded engineering/provider/release boundaries: [verification inventory](../status/capability-delivery-matrix.md). This report is retained evidence, not an active plan.

เพิ่มหน้าตรวจ `/holdings/{holdingSlug}/work/routing` และ API `GET /api/v1/holdings/{holdingSlug}/routing-attention` สำหรับ Company organization-management assignment ปัจจุบัน

## พฤติกรรมที่เพิ่ม

- เลือกตรวจแหล่งข้อมูล named Flow ทั้ง 11 ชนิด: expense, leave, staff_task, request, stock_count, transfer, order, invoice, journal, payroll, record
- อ่านแหล่งข้อมูลเป็นช่วงด้วย keyset และแบ่งหน้าเฉพาะรายการ blocked ที่ผู้ตรวจอ่านและจัดการได้; ไม่ส่งจำนวน/รายละเอียดของรายการที่ไม่มีสิทธิ์
- ไม่จำกัดเอกสารต้นทางไว้เพียง 250 รายการล่าสุด; เอา cutoff นี้ออกจากตัวรวบรวม Work ของ order/invoice/journal/payroll ด้วย เพื่อไม่ให้เอกสารเก่าหายจากคิวหลังแก้ routing
- Expense-linked invoice ใช้ expense เป็นเจ้าของ routing ไม่แสดงเป็น invoice workflow ซ้ำ
- หน้ารายการกดเข้าหน้า review responsibility แล้วใช้ FlowPanel และ command reassign เดิม: expected record/route versions, eligible candidates, reason, audit และ immutable completed decisions
- หน้า read/scan ไม่แก้ผู้รับหรืออนุมัติธุรกรรมเอง; revoke assignment ของผู้ตรวจทำให้ตรวจต่อไม่ได้
- API query: `kind` ตาม flowKinds, `after` เป็น record ID สุดท้ายที่แสดง, `limit` 1–50 (default 25); response มี items/nextAfter/checkedAt และ coverage `named_flow`, Cache-Control private/no-store

## ข้อจำกัดและงานถัดไป

- เป็นการตรวจ named Flow ตามแหล่งที่เลือก ไม่ใช่การรับรองว่าครอบคลุมทุก owner domain; Legal/POS/PMS/overtime/lifecycle/refund/supplier batch ยังใช้ owner queues ของตน ต้องเพิ่ม routing adapter ตามกติกาของแต่ละ owner
- ยังไม่เก็บ first-blocked timestamp, immutable SLA/business-calendar policy, escalation หรือ background scan checkpoints จึงยังไม่ปิดชุด A
- Keyset ไม่ใช่ frozen snapshot เมื่อข้อมูลเปลี่ยนให้ refresh ตรวจใหม่; การ reassign จะตรวจ version/สิทธิ์อีกครั้งเสมอ
- การค้น source ที่มีเอกสารจำนวนมากยังต้องไล่ตรวจสิทธิ์/flow ของเอกสาร; ต้องวัดและเพิ่ม background/indexed scan ก่อนใช้กับข้อมูลระดับ production ขนาดใหญ่ โดยห้ามกลับไปใช้ cutoff ที่ทำให้งานหาย
- Browser acceptance ของหน้าใหม่ยัง pending เพราะ session บน public dev หมดอายุและแสดง login; ไม่ bypass authentication

## หลักฐาน

`tests/routing-attention.test.ts` + `tests/flow-routing.test.ts`: 14 tests passed ครอบคลุม pagination, eligibility repair, revoked primary, manager scope/revocation, reason/history และงานเก่าที่มี owner-created drafts ใหม่อีก 251 รายการ ไม่สร้าง approval ย้อนหลังเพื่อจำลองการผ่าน

`tests/work-cache.test.ts`, `tests/work-refresh.test.ts`, `tests/work-views.test.ts`: 21 tests passed หลังเอา cutoff ออก

ผลเหล่านี้เป็น automated observation บางส่วน ไม่ใช่ sign-off ของ capability หรือ production acceptance
