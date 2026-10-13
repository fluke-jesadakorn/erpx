# คำยืนยันกำหนดส่งจากผู้ขาย — ชุด C / S03

> Recorded report — findings, counts, gaps and hostname observations below belong to the original run. Current workflows: [owner catalogue](../contracts/next/catalogue.md); latest recorded engineering/provider/release boundaries: [verification inventory](../status/capability-delivery-matrix.md). This report is retained evidence, not an active plan.

PO ที่อนุมัติแล้วเป็นเจ้าของจำนวนที่สั่งและรับจริง ส่วนคำยืนยันผู้ขายเป็นหลักฐานเพิ่มเติม ไม่แก้ required date เดิม ไม่สร้างสต็อก และไม่ลงบัญชี

ฝ่ายจัดซื้อเปิด PO แล้วบันทึกเลขอ้างอิงผู้ขาย วันที่ได้รับคำยืนยัน หลักฐาน และจำนวน/วันส่งแยกงวดได้ ทุกครั้งเป็น snapshot ทดแทนแผนปัจจุบันของยอดที่ยังรับไม่ครบ ประวัติเดิมแก้ทับหรือลบไม่ได้ ผู้ขายยืนยันเพียงบางส่วนได้ แต่หน้าจอและ Work ต้องแสดงส่วนที่ยังไม่ยืนยันอย่างชัดเจน

การรับสินค้าผ่าน owner เดิมลดจำนวนคงค้างของคำยืนยันตามวันที่ส่งก่อน–หลัง โดยใช้ปริมาณรับสะสม ณ เวลาบันทึกเป็นฐาน การเปลี่ยนคำยืนยันหลังรับบางส่วนจึงไม่หักของที่รับก่อน snapshot ซ้ำ หากผู้ขายถอนคำยืนยัน บันทึกเป็นประวัติใหม่และคืนยอดค้างทั้งหมดไปเป็น “ยังไม่ยืนยัน” การรับครบหรือยกเลิก PO ปิดงานติดตามจากสถานะ PO โดยคงประวัติคำยืนยันไว้

- ใช้ `commerce.purchase` ใน Company/Branch/Department ของ PO และ current assignment guards; PO ยังต้องผ่าน independent approval เดิม
- การบันทึกนี้หมายถึงพนักงานบันทึกหลักฐานผู้ขาย ไม่ใช่ผู้ขายลงนามใน Portal หรือการอนุมัติราคาใหม่
- คำสั่งตรวจ `expectedVersion` ของ PO ใน transaction เดียวกับ snapshot, source version, receipt, audit, outbox และ notifications การรับสินค้าหรือแก้กำหนดพร้อมกันต้องมีผู้สำเร็จเพียงรายเดียวและอีกคน refresh ข้อมูล
- Work แสดงงานติดตามแก่ทีมจัดซื้อ Inbox แจ้งทีมที่มีสิทธิ์ปัจจุบัน Calendar แสดงงวดส่งที่ยังค้าง พร้อม source link ไป PO
- ทุกการอ่านตรวจ Company/Branch/Department ทั้งเอกสาร คิว และ notification; ไม่ใช้การแจ้งเตือนเพิ่มสิทธิ์

API: `/api/v1/holdings/{holdingSlug}/supplier-promises/{orderId}` — GET อ่านประวัติ/ยอดค้าง, POST บันทึก confirmation หรือ withdrawal ด้วย Idempotency-Key

หลักฐานอัตโนมัติอยู่ใน `tests/integration/procurement/supplier-promises.test.ts`: split/partial receipts, unconfirmed shortfall, replacement/withdrawal, replay/stale/concurrency, revoked/scope guards, immutable history, source completion และ Work/Inbox/Calendar

ยังไม่ปิด RFQ/เปรียบเทียบราคา, supplier portal, การจับคู่ invoice variance, planning/customer delivery promise, business-calendar SLA หรือการส่ง Email ให้ลูกค้า การยืนยันวันส่งจากผู้ขายเพียงอย่างเดียวไม่ใช่หลักฐานว่าลูกค้าจะได้รับสินค้าตามวันนั้น
