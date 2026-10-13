# การจองสต็อกร่วม Sales / POS — ชุด C (จำลองบางส่วน)

> Recorded report — findings, counts, gaps and hostname observations below belong to the original run. Current workflows: [owner catalogue](../contracts/next/catalogue.md); latest recorded engineering/provider/release boundaries: [verification inventory](../status/capability-delivery-matrix.md). This report is retained evidence, not an active plan.

## พฤติกรรมที่เพิ่ม

- Warehouse operator ใช้ `inventory.prepare` จองสินค้าตามบรรทัด Sales order ที่อนุมัติแล้ว พร้อมจำนวน วันหมดอายุไม่เกิน 30 วัน และเหตุผล ผ่านหน้าเอกสารขาย หรือ `POST /api/v1/holdings/{holdingSlug}/commerce/orders/{id}/reservations`.
- Body: `expectedVersion`, `action` (`reserve` / `release`), `lineId`, `quantity` เป็น decimal string, `reason`; reserve เพิ่ม `expiresAt` เป็น UTC epoch milliseconds. ใช้ `Idempotency-Key` และตรวจ active assignment ใน transaction เดิม.
- สิทธิ์ Company/Branch/Department อิงเอกสารต้นทาง การจองไม่ได้แทน approval ของ order และไม่ได้เพิ่มสิทธิ์ส่งของ ผู้ส่งของยังผ่าน owner workflow เดิม.
- Migration `0205_sales_stock_reservations.sql` เพิ่ม `inventory_order_reservations` และ view `inventory_active_reservations` รวม Sales กับ `pos_reservations` เดิม ไม่ย้ายหรือเปลี่ยน ID ของ POS.
- POS catalog, Commerce stock/fulfillment และ transfer availability อ่านยอดจองร่วมกัน DB guard ป้องกันทุก stock writer ลดต่ำกว่ายอดจอง การจองเปลี่ยน stock version เพื่อให้คำสั่งที่อ่านยอดก่อนหน้าตรวจความขัดแย้งได้.
- จองไม่เกินยอดค้างส่ง; การแข่งกันจองและ checkout ต้องสำเร็จได้เพียงเท่าของที่มี ส่งบางส่วน consume เฉพาะ reservation ของบรรทัดต้นทาง ก่อน stock/stock movement/GL ใน transaction เดียวกัน.
- release เก็บจำนวนและเหตุผล; cancel order ปล่อยยอดจองใน transaction ของ cancellation. ไม่คืนสินค้าที่ส่งแล้วด้วย cancellation.
- expiry ใช้เวลาฐานข้อมูลตอน query: หยุดกันของทันทีโดยไม่ต้องรอ worker แต่เก็บ original quantity, consumed/released และ expiry เพื่ออ่านย้อนหลัง.
- คำสั่ง reserve/release เพิ่ม audit, command receipt และ outbox ผ่านระบบ commands เดิม; Work owner ยังคงเป็น order ไม่สร้างสถานะธุรกิจแยก.

## การตรวจ

- `tests/integration/inventory/stock-reservations.test.ts`: Sales สองรายการแย่งของ, source scope, duplicate command, changed payload, stale version, partial/full fulfillment, expiry, cancel และ DB stock guard.
- `tests/integration/pos/pos.test.ts`: เพิ่ม Sales และ POS แข่งจองสินค้าชิ้นสุดท้ายผ่านคำสั่งเจ้าของข้อมูลจริง รวม regression POS เดิม.
- `tests/integration/sales/commerce-returns.test.ts`: กลับสินค้า/บัญชี/คืนเงินยังผ่าน owner เดิม.
- `tests/integration/operations/staff-migration.test.ts`: additive migration ทำซ้ำได้ ตารางใหม่เริ่มว่าง รวม 325 base tables.
- Browser public dev: session ปัจจุบันอ่าน Sales list และเอกสารเดิมได้; fixture ในรายการเป็น fulfilled จึงยังไม่ได้รับรอง reserve/release form หรือการส่งต่อหลายบทบาทผ่าน browser.

## ข้อจำกัดและงานที่ยังต้องปิด

งานนี้ยังไม่ปิด C ทั้งชุด และไม่ใช่ production sign-off: ยังไม่มี adapters ของ Production/Field Service, quality hold/lot/serial/expiry ของสินค้า, transit, shipment/POD และการแจ้งลูกค้า. Reservation expiry ยังไม่มี proactive notification/escalation และยังไม่ได้ทดสอบ browser หลายบทบาท. ผู้ปฏิบัติงานต้องตรวจยอดจองสดบนเอกสารก่อนสัญญาวันส่ง การหมดอายุไม่ได้หมายความว่ามีของเหลือให้จองใหม่เสมอไป.

การประเมินตัวสินค้า expiry แยกจาก reservation expiry ข้างต้น. ไม่อ้างว่าทดสอบสองอย่างนี้แทนกันได้. ไม่มีการเปลี่ยนต้นทุน/จำนวน stock เดิมหรือ backfill การจองในอดีต.
