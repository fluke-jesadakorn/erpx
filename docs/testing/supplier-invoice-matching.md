# Supplier invoice matching — simulation scope

> Recorded report — findings, counts, gaps and hostname observations below belong to the original run. Current workflows: [owner catalogue](../contracts/next/catalogue.md); latest recorded engineering/provider/release boundaries: [verification inventory](../status/capability-delivery-matrix.md). This report is retained evidence, not an active plan.

เพิ่ม PO → receipt ที่ post goods GL แล้ว → supplier invoice evidence → independent Finance review → AP draft → independent GL posting. เริ่มที่ใบรับสินค้าในหน้ารายละเอียด PO; คิว `/supplier-bills`, Work, Inbox และ Calendar ใช้ scope บริษัท/สาขา/แผนกจาก source เดิม.

## วิธีทำงานแยกบทบาท

1. ผู้จัดทำ Finance บันทึกเลข invoice, invoice/due date, หลักฐาน และจำนวน/ราคา/ภาษีครบทุกบรรทัดของ receipt. จำนวนที่ต่างจาก receipt เก็บเป็น discrepancy เพื่อส่งกลับแก้ได้ แต่อนุมัติไม่ได้.
2. Finance reviewer คนละบุคคลกับผู้บันทึกตรวจราคา/จำนวน. ถ้ามีราคาต่างจาก PO ต้องเลือกบัญชี expense ของบริษัทและระบุเหตุผลตาม accounting policy. ผลต่างที่หักล้างกันข้ามบรรทัดยังต้องระบุบัญชีเพื่อรองรับคืนบางสินค้า.
3. คืนเพื่อแก้ไขได้ก่อน post AP; review เดิมไม่ถูกเขียนทับ. หากสร้าง AP draft แล้ว ผู้ review ที่เป็นคนละบุคคลกับผู้บันทึกและผู้สร้าง AP ส่งกลับได้ โดยยกเลิกเฉพาะ draft และคืนสิทธิ์ออก AP จาก receipt ใน transaction เดียว. เก็บ invoice ID เดิมและประวัติไว้ บันทึกฉบับแก้จาก receipt พร้อม version ปัจจุบัน. เลข AP ภายในไม่ซ้ำกัน ส่วนเลขบิลผู้ขายคงอยู่ใน matching evidence แม้ฉบับแก้ใช้เลขอ้างอิงเดิม. หลัง post ต้องใช้ adjustment/return ไม่ใช้ทางนี้.
4. ผู้รับผิดชอบขั้น invoicing ที่ PO ระบุไว้สร้าง AP จาก snapshot ที่ผ่าน review. การสร้าง AP และ link review สำเร็จ/ย้อนกลับด้วย transaction เดียว.
5. Finance ผู้มีสิทธิ์โพสต์ที่เป็นคนละคนกับผู้สร้าง AP ตรวจ GL preview แล้ว post ในงวดเปิด. ระบบ post original receipt clearing + price variance + input tax = actual AP. Stock คงต้นทุน receipt เดิม; **ไม่ใช่ landed cost/revaluation**.
6. Supplier return ใช้ actual invoice subtotal/tax ใน debit note และ original receipt cost ใน stock reversal; partial return มี cumulative rounding และย้อน variance เดิม. รายการจ่ายแล้วใช้ refund obligation flow เดิม.

## Guards และหลักฐาน

- Idempotency, expectedVersion, current assignment, independent reviewer และ named PO invoicing owner.
- Receipt มี matching history แล้วสร้าง invoice ข้าม approved review ไม่ได้. Reference ซ้ำของ supplier/company หรือ active match ซ้ำ receipt ไม่ได้. Legacy receipt ที่ไม่เคยเข้า matching ยังใช้ API เดิมได้; ยังไม่มี company policy บังคับ matching ทุก receipt.
- Header content/line/event evidence immutable. การ reject/return เก็บ history; ไม่ rewrite receipt หรือ GL เดิม.
- บิลที่ส่งกลับมีงานขอฉบับแก้ใน Work ของ named PO invoicing owner; เมื่อส่งฉบับแก้แล้ว งานเดิมออกจากคิวแต่เก็บประวัติ. ผู้รับหมดสิทธิ์แสดง blocker ให้เปลี่ยนผู้รับผ่าน routing เดิม.
- Review queue ตรวจจำนวนผู้มีสิทธิ์ปัจจุบัน; ถ้าไม่มี independent Finance reviewer แสดง blocker ให้ administrator แก้ role coverage. ขั้น AP ใช้ routing ของ PO เดิม.
- Automated: `tests/integration/procurement/supplier-bill-matching.test.ts` ตรวจ variance บวก/ลบ, partial/final returns กระทบยอดทุกบัญชี, quantity correction, invoice bypass, immutable evidence, concurrent capture/review, revoked role, scope/notifications/Calendar, closed period และ idempotent posting.

## สิ่งที่ยังไม่ครอบคลุม

หนึ่ง supplier bill ต่อ receipt และต้องครบทุกบรรทัด; ยังไม่มี invoice รวมหลาย receipts/PO, partial invoice ของ receipt เดียว, free-value lines, FX/multi-precision, landed-cost allocation หรือ tolerance policy อัตโนมัติ. ใช้ currency และ precision 2 ตำแหน่งตาม commerce contract เดิม ไม่ตีความเอกสารเก่าใหม่. Receipt journal ต้องเป็นรูปแบบสองบรรทัดเดิม; รูปแบบอื่นหยุดให้ review.

หลักฐานจำลองไม่ใช่ business sign-off หรือการรับรองนโยบายบัญชี. Browser/provider/UAT แยกจาก automated tests; ไม่ถือว่าเสร็จ S03 หรือชุด B/C ทั้งหมด.

Correction handoff: Work ใช้ named PO owner; Calendar แบบ team แสดงงานแก้ไขล่าสุดและซ่อนเมื่อส่งฉบับใหม่. Calendar mine ยังอิงผู้บันทึก/ผู้ review ไม่ใช่ named owner ทุกกรณี. Inbox แสดงข้อความตามสถานะ source. [Matching/Calendar regression 20 tests](evidence/2026-09-20-bill-correction-calendar.txt) ผ่าน; browser correction acceptance ยัง pending.
