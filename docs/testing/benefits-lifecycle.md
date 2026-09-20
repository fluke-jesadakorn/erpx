# Benefits lifecycle simulation

สถานะนี้เป็น **owner implementation บนข้อมูลจำลอง** สำหรับ S18/G16 ยังไม่ใช่ provider, payroll expert, production หรือ business sign-off

## Flow ที่ส่งมอบ

1. ผู้ดูแลสร้าง plan พร้อม provider, coverage, currency, deduction, employer cost และ effective dates; ผู้สร้าง publish/reject เองไม่ได้
2. ผู้ดูแลสร้าง enrollment window และผู้ตรวจอีกคนเปิดใช้งานตาม version ที่เห็น
3. พนักงานเพิ่ม dependent ของตนเอง ผู้ตรวจปัจจุบัน verify ก่อนใช้กับ family coverage
4. พนักงานเลือก published plan ภายใน open window ระบบ snapshot plan/window version, dependents และยอดเงิน
5. พนักงาน submit จาก Work; ผู้ดูแลบันทึก provider acknowledgement reference; ผู้รับรองอีกคน release payroll input
6. Payroll preview อ่านเฉพาะ released input ที่ currency และ effective period ตรงกัน แล้วรวม employee deduction และ employer cost ใน calculation และ immutable input snapshot

ทุก transition ใช้ idempotency key, optimistic version, current permission และ immutable event history. Source fields ของ election และ released payroll input เปลี่ยนหรือลบไม่ได้

## หลักฐานจำลอง

- `tests/benefits.test.ts` ตรวจ independent review, dependent eligibility, stale version, provider acknowledgement, independent payroll release, immutable history และ payroll totals/snapshot
- `tests/payroll-core.test.ts` ยืนยัน regression ของ payroll core เดิม
- `tests/staff-migration.test.ts` replay migration ทั้งหมดและรอบ idempotent
- ผลรัน: `docs/testing/evidence/2026-09-20-benefits-tests.txt`

## Gate ที่ยังเปิดอยู่

- provider file/API acknowledgement จริง, rejected/changed enrollment จาก provider และ reconciliation
- migration/open enrollment ของบริษัทจริง, plan documents, eligibility policy และ privacy/retention review
- Thai payroll expert review และ shadow payroll สองรอบ
- browser หลายบทบาท, customer/business UAT และ production sign-off

