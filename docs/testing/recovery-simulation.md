# ซ้อมกู้คืนฐานข้อมูลธุรกิจจำลอง — J บางส่วน

> Recorded report — findings, counts, gaps and hostname observations below belong to the original run. Current workflows: [owner catalogue](../contracts/next/catalogue.md); latest recorded engineering/provider/release boundaries: [verification inventory](../status/capability-delivery-matrix.md). This report is retained evidence, not an active plan.

คำสั่ง: `bun run test:integration tests/integration/platform_administration/recovery-simulation.test.ts`

ใช้ Bun 1.2.17 / Node 24.20.0 ตาม test wrapper และ `pg_dump` / `pg_restore` PostgreSQL 18.6 บน PATH. ต้องมี `ERPX_TEST_ADMIN_URL` ตามข้อกำหนด test environment เดิม; ไม่แสดง credential ใน arguments หรือ log.

## สิ่งที่ทำและตรวจ

1. สร้าง isolated test databases ด้วย role `erpx_test_admin` ที่ไม่เป็น superuser และใช้ชื่อ `erpx_test_*` บน loopback 5432 เท่านั้น.
2. สร้าง master/opening-stock fixture แล้วใช้ owner commands สร้าง Sales order, submit, independent approval, reserve และส่งบางส่วน. ไม่มีการสร้าง delivery หรือ reservation ปลายทางข้าม flow.
3. เก็บจำนวนแถวและ SHA-256 ของเนื้อหาทุก base table ก่อน dump รวม journal lines, stock movements, reservations, command receipts, audit และ outbox.
4. `pg_dump --format=custom` แล้วสร้างฐานปลายทางชั่วคราวใหม่ จากนั้น `pg_restore --single-transaction --exit-on-error`.
5. เทียบทุก base table ว่าจำนวนและ checksum ตรงกัน และตรวจ partial order version, stock quantity/value และยอดจองคงเหลือ. ลองลดสต็อกต่ำกว่ายอดจองแล้วต้องถูก trigger ปฏิเสธ แสดงว่ากติกาฐานข้อมูลถูกกู้กลับมาด้วย.
6. ปิด connection และลบเฉพาะฐานปลายทางที่สร้างเอง โดยตรวจชื่อและ database owner ก่อนลบ; ลบ archive จำลองเมื่อจบ. ไม่ใช้ `--force` กับฐานที่มี connection ค้าง.

ผลการรันเขียนใน ignored `.scratch/cross-department-audit/recovery-latest.json`: source IDs, dump checksum, digest ของทุกตาราง และเวลาสร้าง/restore/verify ฐานปลายทาง. หลักฐานที่เลือกเก็บถาวรอยู่ใน `docs/testing/evidence/` พร้อม checksum ใน ledger.

## ขอบเขตที่ยังไม่รับรอง

นี่คือ logical restore ของหนึ่ง Holding database บนข้อมูลจำลองขนาดเล็ก ไม่ใช่ production recovery sign-off. ยังไม่รวม control/read databases, private files, secret/config recovery, job workers ที่กำลังทำงาน, remote backup, WAL/PITR, provider receipts ที่เกิดหลัง snapshot หรือ load ระดับ production. ไม่ใช้เวลาใน simulation นี้รับรอง RTO ≤4 ชั่วโมง และไม่อ้าง RPO ≤15 นาทีจาก logical dump.

ต้องทำ full recovery drill ตาม [runbook](../../ops/runbooks/nextjs-recovery.md) บน environment แยกที่สะท้อน production แล้วให้เจ้าของบริการรับรองก่อน cutover. Full regression และ readiness dashboard ไม่ปลด gate นี้.
