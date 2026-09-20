# E-commerce lifecycle simulation

## ขอบเขตที่ implement แล้ว

E-commerce เป็นช่องทางรับคำสั่งซื้อและเก็บ source link ส่วน Sales, Inventory, Finance และ Returns ยังคงเป็นเจ้าของธุรกรรมของตนเอง ระบบรองรับ channel draft → independent activation, listing draft → independent publication, version-locked cart → checkout → Sales order submission → independent order approval → shared stock reservation → fulfillment link → invoice-gated return request

ทุก write command ใช้ `Idempotency-Key`, current assignment, Company scope และ optimistic version. Channel/listing creator อนุมัติงานตนเองไม่ได้ ราคากับ listing version ถูก snapshot ลง cart และ checkout ซ้ำไม่สร้าง Sales order ซ้ำ

## หลักฐานจำลอง

- `tests/ecommerce.test.ts` ตรวจ independent review, stale source, idempotent checkout, Sales owner handoff, shared reservation, fulfillment/stock quantity และ return ที่ไม่ข้ามเงื่อนไข posted invoice
- `tests/commerce-returns.test.ts` ตรวจ return/credit/refund และ source-linked accounting หลัง fulfillment ถูก invoice แล้ว
- `tests/stock-reservations.test.ts` ตรวจการแข่งขันสินค้าชิ้นสุดท้าย, partial fulfillment, release/expiry และป้องกัน stock ต่ำกว่ายอดจอง
- migration `0234_ecommerce.sql` เพิ่ม source records และ immutable fulfillment/return links แบบ additive

## ขอบเขตที่ยังเป็น acceptance gate

Storefront จริง, payment provider, tax/shipping integration, browser หลายบทบาท, customer UAT และ production sign-off ยังไม่ผ่าน การเปลี่ยน capability เป็น Implemented หมายถึงมีพฤติกรรมในระบบและ automated evidence แล้ว ไม่ได้หมายถึง scenario S16 หรือการเปิดใช้จริงผ่านครบ
