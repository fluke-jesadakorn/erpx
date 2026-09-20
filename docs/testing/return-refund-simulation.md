# คืนสินค้าและคืนเงิน — simulation acceptance S04

วันที่ 2026-09-20 · ครอบคลุมบางส่วนของช่วง 1 / G04 และ G12 · ยังไม่ใช่ production sign-off

## ขอบเขตที่ implement

`Commerce return → stock/credit GL review → invoice adjustment + refund obligation → independent approval → Treasury instruction → CSV export → human bank action → imported bank evidence → cash GL + bank reconciliation`

- Invoice ที่ยังค้าง ชำระบางส่วน หรือชำระครบสามารถขอคืนได้ตามจำนวนที่ส่งจริง หักคำขอคืนก่อนหน้าที่ยังไม่ถูก reject
- ตอน complete credit/debit note ใช้ `credited = min(return total, current outstanding)` และ `refund = return total - credited`; ปรับ invoice และสร้าง refund obligation ใน transaction เดียว พร้อมตรวจ invoice version ล่าสุด
- ตัวอย่าง invoice 20.00 ชำระแล้ว 15.00 และคืนทั้งหมด: ลดหนี้ 5.00 สร้าง obligation 15.00 ไม่ reverse receipt เดิม
- Customer return เป็น outgoing refund; supplier return เป็น incoming refund; source return/invoice/party/chart/currency/balance account ผูกกับ obligation เดียว
- คืนมูลค่าสต็อกตามบัญชีจาก journal ต้นทาง ไม่เลือกบัญชีใหม่จาก configuration ปัจจุบัน ส่วน credit/debit ใช้บัญชีจาก invoice ต้นทาง; รายการเดิมที่ไม่สามารถระบุคู่บัญชีต้นทางชัดเจนหยุดให้ตรวจ
- ผู้อนุมัติ refund ต้องต่างจากผู้ขอคืนและผู้ complete note; Treasury ผู้ดำเนินการต้องต่างจากผู้สร้าง obligation และผู้อนุมัติ
- คำสั่งจ่ายบางส่วนกันยอดทันที; ผลรวม prepared/exported/unknown ต้องไม่เกินยอดเงินคงเหลือ; optimistic guards + serializable transaction ป้องกันการแข่งจองยอดสุดท้าย
- Export มี SHA-256, แถวละ source/instruction/currency/amount/beneficiary และเก็บ command receipt ที่เรียกดูซ้ำได้; export ไม่บันทึกเงินสดหรือเปลี่ยนสถานะเป็นจ่ายแล้ว
- Confirm ต้องมี imported statement line ที่ตรง Company, bank ledger account, currency, amount, direction, date และ version; สร้าง payment/posted GL ลด obligation และจับคู่ bank line ใน transaction เดียว
- การยืนยันซ้ำด้วย key เดิมคืนผลเดิม ไม่ repost; bank line ใช้ซ้ำไม่ได้; งวดปิดไม่อนุญาต GL
- `unknown` คง reservation ไว้; ผู้ตรวจ Finance คนอื่นต้องบันทึกหลักฐาน bank failure ก่อนปล่อยยอด; cancel ได้เฉพาะก่อน export
- การ reverse payment เดิมที่มี return เกี่ยวข้องถูกหยุดเพื่อไม่สร้างยอดหนี้กลับซ้ำกับ note/refund; การแก้หลังยืนยันต้องออกแบบ compensating source workflow ต่อ ไม่แก้ journal ทับ

## หน้าจอและการรับช่วง

| ผู้รับ | จุดเข้า | การทำงาน |
| --- | --- | --- |
| Sales/Purchasing | Order → Receipts/deliveries → Request return | เลือกจำนวน/วันที่/เหตุผลจาก fulfillment เดิม; ผู้จัดการคนอื่น approve/reject |
| Finance reviewer | Return → stock GL / credit/debit GL | ตรวจและ post ทั้งสอง journal ก่อน complete note; หลัง complete แสดงยอด credit กับ refund แยก |
| Independent Finance approver | Finance → Return refunds, Work, Inbox | เปิด source order/invoice ตรวจยอดและบัญชีคู่ค้าแล้วอนุมัติ |
| Company Treasury | Work / Return refunds | เตรียมคำสั่งบางส่วนหรือสุดท้าย export ตรวจ checksum แล้วปฏิบัติงานธนาคารด้วยคน |
| Company Treasury | Refund instruction / Calendar | ยืนยันด้วย statement ที่ import ผ่าน Finance/Bank หรือแจ้งผลไม่ชัดเจน |
| Independent Finance reviewer | Work / Inbox | ตรวจหลักฐาน failure ก่อนปล่อย reservation ให้เตรียมใหม่ |

Inbox อ้างอิง owner record และตรวจสิทธิ์ซ้ำก่อนเปิดเผยข้อความ; Calendar เพิ่ม layer `refunds` แสดง planned date ตาม instruction โดยไม่สร้างสถานะธุรกิจใหม่; ผู้ไม่มี `finance.read` ไม่เห็นข้อมูล refund ใน adapter ทั้งสอง ส่วน Treasury bank evidence จำกัด Company assignment

## คำสั่งและหลักฐานอัตโนมัติ

```sh
bun run test tests/commerce-returns.test.ts
bun run test tests/finance-payment-corrections.test.ts tests/work-cache.test.ts tests/work-refresh.test.ts
bun run test tests/calendar.test.ts tests/calendar-api.test.ts tests/work-views.test.ts
bun run typecheck
bun run lint
bun run deploy:build
```

`tests/commerce-returns.test.ts` สร้าง fixture ผ่าน owner services จริง: PO/SO → fulfillment → post goods GL → invoice → payment → return → credit → refund → bank. ครอบคลุม unpaid supplier return, paid/part-paid customer และ supplier return, partial/final refund, replay และ source-key mismatch, over-refund, closed period, self approval/payer, company/branch isolation, unknown/failure review, concurrent instructions, balanced source-account GL และ no duplicate bank match พร้อม Work/Inbox/Calendar visibility

ผลตรวจแต่ละรอบอยู่ใน [progress register](cross-department-implementation-progress.md); ไม่ถือว่ามี test path เท่ากับผ่าน browser/provider acceptance

## Runbook ข้อมูลจำลองและ recovery

1. ใช้ `bun run test` เพื่อสร้างฐาน `erpx_test_*` แยกผ่าน isolation guard ห้ามเปลี่ยน administrator URL ไปชี้ production
2. Approval ต้องตรวจชื่อ/บัญชีคู่ค้าและ source evidence; ข้อมูลทดสอบใช้ชื่อ Simulation/TEST เท่านั้น ห้าม export ไปจ่ายจริง
3. เมื่อ response หาย ให้ส่ง command เดิมพร้อม key/input เดิม หรือเปิด instruction แล้ว Retrieve export; ตรวจผลก่อนสร้างคำสั่งใหม่
4. เมื่อธนาคาร timeout ให้ mark unknown ไม่ใช่ failed แล้วตรวจ statement/รายการธนาคารจริงก่อนตัดสินใจ
5. เมื่อ statement ยังไม่เข้า หรือ currency/account/amount/date ไม่ตรง ให้ค้างไว้และแก้หลักฐาน ห้ามเลือก bank line อื่นเพื่อให้ workflow ผ่าน
6. เมื่อ finance period ปิด ให้ Finance ทบทวน period policy ก่อนดำเนินการ; ระบบไม่เปิดงวดหรือเปลี่ยน payment date อัตโนมัติ
7. Application rollback ไม่ลบ obligation/instruction/payment หรือ receipt; migration เป็น additive และไม่ backfill ประวัติ approval/เงินคืนเก่า

## Gate ที่ยังไม่ผ่าน

- Browser หลายบทบาทบน fixture เดียวตั้งแต่ approve ถึง bank confirmation; รอบนี้ตรวจ public dev list/navigation เท่านั้น
- CSV เป็น instruction register สำหรับเจ้าหน้าที่ ไม่ใช่รูปแบบอัปโหลดของธนาคารใด; ต้องรับรองรูปแบบ bank-specific แยกหากต้องการ
- รวม batch สำหรับ AP/Payroll, เงินคืนผ่าน Stripe และการแก้ไขหลังยืนยันยังไม่ครอบคลุมด้วย service นี้
- ยังใช้ money scale สองตำแหน่งตามเอกสารเดิม และชำระผ่าน cash/bank account ที่ configure สำหรับ Commerce; การรับรู้ FX gain/loss กับสกุลเงินอื่นต้องทำต่อ
- ยังไม่มี customer Portal/Email แจ้งผล, quality disposition ของสินค้าคืน, SLA/escalation ที่ตั้งตามบริษัท, หรือผู้รับรองธุรกิจจริง
- เอกสารและยอดก่อน migration ไม่ถูกตีความใหม่: `credited_minor/refund_minor` ของ return เก่ายังคง null เพื่อให้ตรวจและกระทบยอดแยก
