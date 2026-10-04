# RFQ → supplier comparison → independent award → draft PO

> Recorded report — findings, counts, gaps and hostname observations below belong to the original run. Current workflows: [owner catalogue](../contracts/next/catalogue.md); latest recorded engineering/provider/release boundaries: [verification inventory](../status/capability-delivery-matrix.md). This report is retained evidence, not an active plan.

ขอบเขตจำลองชุด C/S03: ฝ่ายจัดซื้อระบุ demand/หลักฐาน คลัง วันต้องการ และ product/quantity/UOM; บันทึกข้อเสนอครบทุกบรรทัดจากผู้ขายแต่ละรายในสกุลเงิน Company; เปรียบเทียบราคารวม ภาษี วันส่ง และวันหมดอายุ; เสนอผู้ชนะพร้อมเหตุผลรวมการยอมรับส่งช้า; ผู้จัดการคนอื่นอนุมัติ; สร้าง PO draft ใน transaction เดียวกับ RFQ handoff แล้วดำเนิน workflow PO เดิมต่อ

เริ่มจาก Sales & purchasing → Purchase orders → Supplier quotations & award reviews. Work มีคิว `Supplier quotation reviews`; ผู้สร้าง/ผู้เสนอ/ผู้บันทึกข้อเสนอที่เลือกไม่มีสิทธิ์อนุมัติ award ของตัวเอง แม้สลับ role. Inbox และ Calendar ตรวจ scope จาก RFQ owner; คิวไม่มีผู้มีสิทธิ์แสดง blocked reason ให้ Company administrator ซ่อม role coverage

- RFQ demand lines, supplier offers, offer lines และ decision events เป็น append-only. ข้อเสนอใหม่ของผู้ขายเดียวกันแทน comparison ปัจจุบัน แต่เก็บราคาและหลักฐานเก่า
- ระหว่าง review/approved เพิ่มข้อเสนอไม่ได้ ต้อง return พร้อมเหตุผลก่อน; return ล้าง award ปัจจุบันแต่คง events. ยกเลิก RFQ ก่อน PO ได้; หลังสร้าง PO ใช้ cancellation ของ owner PO
- Approval เป็นการรับรอง selection ไม่ใช่ PO approval และไม่ส่งคำสั่งซื้อไปผู้ขาย. PO ที่สร้างยังเป็น draft ต้องให้คนอื่นอนุมัติ
- PO supplier/warehouse/required date/product/quantity/price/tax ต้องตรง award; คำสั่งสร้างหรือแก้ draft ไม่สามารถละ RFQ ID เพื่อเปลี่ยนเงื่อนไขที่ผูกแล้วได้. หลังผูกต้องยกเลิก PO และเริ่ม RFQ ใหม่ถ้าจะเปลี่ยนเงื่อนไข
- Idempotency/version/current permission ใช้ command boundary เดิม; RFQ update, PO, source links, audit, receipt, outbox และ notices commit/rollback ด้วยกัน. ไม่สร้าง stock, AP หรือ GL จาก quotation/award
- รายการใช้ precision เงินสองตำแหน่งตาม commerce owner ปัจจุบันและ snapshot currency; ยังไม่ใช่การปิด multi-currency/FX. การตรวจ quote expiry ใช้ UTC date ตาม runtime ปัจจุบัน ไม่ใช่ business-calendar policy

## หลักฐานและสิ่งที่ยังเหลือ

`tests/procurement-rfqs.test.ts` ตรวจ comparison, independent approval, return/replacement history, stale/replay/race, scope/revocation, immutable offers, changed award terms, PO normal approval, Inbox/Calendar และ missing reviewer coverage; `tests/staff-migration.test.ts` ตรวจ complete/idempotent baseline 337 tables

เพิ่ม PR→RFQ แบบจับคู่ทุกรายการกับ product/quantity เดิมแล้ว: เริ่มจากคำขอที่อนุมัติและผู้รับเริ่มงานแล้ว เก็บ source version/evidence version/items snapshot ป้องกัน RFQ ซ้ำและห้ามข้าม award ไปสร้างหรือ link PO อื่น; PR→RFQ→PO commit พร้อมกันโดยตรวจ approval/handler ปัจจุบัน การเพิ่ม comment ใช้ current version โดยไม่เปลี่ยน evidence snapshot.

ยังไม่ปิด partial/split requisition allocations, UOM conversion และ split awards หลาย PO, supplier portal/email/RFQ invitation, private evidence attachments, supplier qualification, invoice variance matching, Company SLA หรือ live provider/UAT. RFQ ที่เริ่มจากหน้าทั่วไปยังใช้ข้อความ demand evidence; RFQ ที่เริ่มจากคำขอมี typed source link. ไม่แปลงข้อความอ้างอิงเก่าเป็นคำขอที่อนุมัติย้อนหลัง. RFQ list แสดงล่าสุด100รายการพร้อมระบุขอบเขตบนหน้า; full search/pagination ยังเหลือ

API ใหม่: `/api/v1/holdings/{holdingSlug}/procurement-rfqs` และ `/{id}`. Handoff ใช้ API create PO เดิมพร้อม `rfqId`/`rfqVersion` และ requisition IDs/versions additive; API/ID/ประวัติ PO เดิมยังใช้ได้
