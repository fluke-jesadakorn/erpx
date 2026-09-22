# ERPX — ความคืบหน้าการพัฒนา flow ข้ามแผนก

วันที่ 2026-09-22 · สถานะ: กำลังพัฒนา ยังไม่ครบแผนช่วง 0–8

## Finance browser acceptance — 2026-09-22

ปิด browser acceptance ของชุด Finance ในบริษัทจำลองแบบแยกฐานข้อมูลครบ 3 เส้นทาง: controlled payment ที่ใช้บัญชี Finance preparer, Finance approver และ Treasury แยกกัน; period close สองงวดต่อเนื่องที่ผู้เตรียมเริ่มและผู้อนุมัติอิสระปิด January/February 2027; และ opening balance import ที่ผู้เตรียม preview/create แล้วผู้อนุมัติคนละคน post. ผลลัพธ์ยังคงอยู่หลัง reload และตรวจผ่าน owner API ว่ายอดคงเหลือ, bank match, journal และสถานะงวดตรงกับ record ต้นทาง.

คำสั่ง `bun run demo:verify:browser` ผ่าน 3/3 ใน run `b9dbfdf4fee9` ใช้เวลา 30.4 วินาที. เพิ่มการรอ state `closing` จาก API ก่อนเปิด approval workspace เพื่อให้การทดสอบสะท้อน handoff ที่บันทึกจริงและไม่รับ state เก่าจากหน้าเว็บ. หลักฐานอยู่ที่ [Finance browser acceptance](evidence/2026-09-22-finance-browser-acceptance.txt).

ขอบเขตยังเป็น automated browser acceptance ของ working demo company ไม่ใช่ business sign-off, live bank/provider, statutory payroll หรือ production certification; demo reset แยกจากข้อมูลบริษัทจริง.

## Procurement RFQ browser acceptance — 2026-09-22

เพิ่ม browser acceptance ของ Release 1 สำหรับ sourcing ที่เชื่อมกับ PO: Purchasing manager สร้าง RFQ จากแบบฟอร์มจริง, บันทึก quotation ครบทุก demand line, ส่งเหตุผล award, Purchasing manager อีกคนอนุมัติ และผู้สร้างสร้าง awarded draft PO ต่อได้. ตรวจสถานะ RFQ เป็น `ordered` จาก API หลัง handoff เพื่อยืนยันว่าการเปลี่ยนแปลงอยู่ในฐานข้อมูลจริงและไม่ใช่เพียงข้อความบนหน้าเว็บ.

คำสั่ง `bun run demo:verify:browser` ผ่าน 4/4 ใน run `b848b9a1229d` ใช้เวลา 37.3 วินาที โดยเพิ่ม procurement flow เป็น test ที่สี่. หลักฐานอยู่ที่ [Procurement RFQ browser acceptance](evidence/2026-09-22-procurement-rfq-browser-acceptance.txt).

ขอบเขตยังเป็น isolated demo acceptance ไม่ใช่ supplier/provider, production operation หรือ business sign-off; การสร้าง PO ยังต้องผ่าน approval/receiving/AP workflow ต่อไป.

## Source-linked purchase request → partial RFQ browser acceptance — 2026-09-22

เพิ่ม browser acceptance ที่เริ่มจากใบขอซื้อจริงใน working demo: employee submit → people manager approve → Finance preparer และ Finance approver อนุมัติแยกกัน → Purchasing assign/start work → จับคู่ demand 6 จาก 10 หน่วยกับสินค้าและคลัง → สร้าง RFQ ที่เก็บ source request link → บันทึก quotation และ award → purchasing manager อีกคนอนุมัติ → สร้าง awarded draft PO. หลัง handoff ตรวจ owner API ว่า RFQ เป็น `ordered` และยังผูกกับ request เดิม.

รัน `bun run demo:verify:browser` ใน run `53ef5742a0b6` ผ่าน 5/5 ใช้เวลา 47.8 วินาที โดย source-linked flow ใช้ 17.3 วินาที. หลักฐานอยู่ที่ [source-linked purchase request RFQ browser acceptance](evidence/2026-09-22-purchase-request-rfq-browser-acceptance.txt) และบันทึกใน evidence ledger แล้ว.

ขอบเขตนี้ยืนยันเฉพาะ partial allocation 6/10 และ draft-PO handoff; remaining four-unit split, receipts/AP/payment ต่อเนื่อง, supplier/provider, production และ business sign-off ยังเป็น gates แยก.

## Source-linked purchase request → posted AP browser acceptance — 2026-09-22

ขยาย flow เดิมผ่านหน้าจอจริงต่อจาก draft PO: buyer ส่ง PO และ purchasing manager อนุมัติ, warehouse รับสินค้า 6 หน่วย, Finance approver เปิดและ post goods movement journal, Finance preparer จับคู่ supplier invoice กับ receipt ที่ post แล้ว, Finance approver ตรวจ match แบบอิสระ, จากนั้นเตรียมและ post AP invoice ในงวดบัญชี 2026. Owner API ตรวจซ้ำว่า RFQ เป็น `ordered` และยังผูกกับ purchase request เดิม.

รัน `bun run demo:verify:browser` ใน run `263a5fce0f12` ผ่าน 5/5 ใช้เวลา 57.8 วินาที; source-linked flow ใช้ 26.1 วินาที. หลักฐานอยู่ที่ [source-linked purchase-to-AP browser acceptance](evidence/2026-09-22-purchase-request-purchase-to-ap-browser-acceptance.txt) และบันทึกใน evidence ledger แล้ว.

ขอบเขตนี้ยืนยัน PO ที่จัดสรร 6/10, receipt ที่ post GL แล้ว และ AP invoice ที่ post แล้วหนึ่งใบ; remaining four-unit split, supplier payment/export, provider, production และ business sign-off ยังเป็น gates แยก.

## Source-linked purchase request → supplier payment browser acceptance — 2026-09-22

ขยายเส้นทางเดียวกันผ่านปลายทาง Finance จริงใน working demo: หลังโพสต์ AP invoice แล้ว Finance preparer สร้าง supplier payment batch, Finance approver ตรวจและอนุมัติ beneficiary/amount, Treasury export คำสั่ง, Finance preparer import sandbox bank statement และ Treasury เลือก debit ที่ตรงกันเพื่อ settle invoice. การทดสอบใช้ browser contexts แยก employee, people manager, Finance preparer, Finance approver, purchasing operator, purchasing manager, warehouse manager และ Treasury; หลัง reload ตรวจ batch item เป็น `confirmed`, payment/allocation link และ API ของ invoice เป็น `paid` พร้อม outstanding เป็นศูนย์.

คำสั่ง `ERPX_DEMO_RUN=57b31d4eaffa ERPX_E2E_BASE_URL=http://127.0.0.1:3102 bunx playwright test tests/e2e/demo-purchase-request-rfq.spec.ts --project=desktop` ผ่าน 1/1 ใน 46.6 วินาที (Playwright summary 47.2 วินาที). หลักฐานอยู่ที่ [purchase request → payment browser acceptance](evidence/2026-09-22-purchase-request-purchase-to-payment-browser-acceptance.txt).

ขอบเขตยังเป็น isolated simulation ไม่ใช่ supplier/bank provider, statutory procurement, production operation หรือ business sign-off; browser ใช้ branch รับของ 6/10 ส่วน split allocation และ receipt 6+4 มี owner regression แยกต่างหาก.

## Source-linked split purchase request → full receipt and supplier payment browser acceptance — 2026-09-22

ปิด browser acceptance ของตัวอย่างเดียวกันให้ครบยอด 10 หน่วยผ่านหน้าจอจริง: Purchasing แบ่ง demand เป็น RFQ/PO สองชุด (6 + 4), ได้ quotation และ purchasing-manager approval แยกกัน, Warehouse รับของครบทั้งสองใบ, Finance approver โพสต์ goods-movement GL สอง journal, Finance จับคู่และโพสต์ AP invoice แบบ receipt-specific สองใบ (THB 150,000 และ THB 100,000), Finance รวมสอง invoice ใน supplier payment batch THB 250,000, Treasury export และยืนยัน bank debit จาก statement sandbox สองรายการ. หลัง reload ตรวจ instruction ทั้งสองเป็น `confirmed` และ API ของ invoice ทั้งสองเป็น `paid` พร้อม outstanding เป็นศูนย์.

คำสั่ง `ERPX_DEMO_RUN=d48f9000053f ERPX_E2E_BASE_URL=http://127.0.0.1:3102 bunx playwright test tests/e2e/demo-purchase-request-rfq.spec.ts --project=desktop` ผ่าน 1/1 ในประมาณ 1 นาที. หลักฐานอยู่ที่ [split purchase request → payment browser acceptance](evidence/2026-09-22-purchase-request-split-purchase-to-payment-browser-acceptance.txt) และบันทึกใน evidence ledger แล้ว. Demo seed เพิ่ม default Finance invoice reviewer เพื่อไม่ให้เอกสาร AP ใหม่ค้างโดยไม่มี primary owner.

ขอบเขตยังเป็น isolated simulation ไม่ใช่ supplier/bank provider, statutory procurement, production operation หรือ business sign-off; ใบแจ้งหนี้ยังเป็นสองใบผูกกับ receipt แต่ละ PO และการรวมเป็น supplier invoice เดียวข้ามหลาย PO ยังคงเป็น capability gate แยก.

## Expense browser acceptance — 2026-09-22

ปิดการตรวจ browser ของบริษัทจำลองสำหรับ expense workflow 4/4 กรณี: draft ที่ข้อมูล People ยังไม่ครบและคงอยู่หลัง reload, claimant → department reviewer → Accounts preparer → Accounts approver → GL posting → Treasury จ่ายสองหลักฐาน, IT สร้าง AI provider draft ที่ mask secret และ employee ถูกปฏิเสธ, และ AI receipt suggestions ที่เติมเฉพาะช่องว่างพร้อมให้ผู้ใช้แทนค่าด้วยตนเองเมื่อ source เปลี่ยน. Fixture แยกติดตั้ง staff levels/positions/reporting lines เพื่อให้ route ใช้กติกา organization จริง ไม่ bypass validation. หลักฐานอยู่ที่ [expense browser acceptance](evidence/2026-09-22-expense-browser-acceptance.txt).

ยังเป็น local/demo acceptance: provider, bank, payment, statutory payroll และ company sign-off จริงยังไม่ผ่าน และไม่ยกระดับเป็น production หรือ live-provider verification.

## Field Service warranty-to-Finance handoff — 2026-09-22

เพิ่ม warranty pricing ที่ใช้งานจริงใน service agreement โดยเก็บส่วนลดแบบ immutable และคำนวณจาก `warranty_end` ของ asset ฝั่ง server. งานที่อยู่ใน warranty สร้าง billing release และ Finance receivable ตามจำนวนหลังส่วนลด พร้อมเก็บหลักฐานว่าใช้ policy ใด; งานที่หมด warranty คิด visit rate เต็ม. Reviewer คนละคนปล่อย billing แล้ว Finance approver คนละคน post invoice ต่อใน workflow Finance ปกติ โดยยังคง source linkage ของ service order.

`bun run test -- tests/field-service.test.ts` ผ่าน 5/5 รวม chargeable path, warranty 50% (150000 → 75000), invoice source linkage และ posting. หลักฐานอยู่ที่ [Field Service warranty pricing](evidence/2026-09-22-field-service-warranty-pricing.txt). Browser หลายบทบาท, provider และ business/UAT acceptance ยังคงเป็น gate แยก.

## Purchase-to-payment acceptance — 2026-09-21

เพิ่ม owner-service acceptance ที่เริ่มจากใบขอซื้อ Laptop 10 เครื่องและเดินครบผ่านการอนุมัติอิสระ, RFQ/award, PO, รับของ 6+4, post stock GL สองชุด, match ใบแจ้งหนี้ผู้ขายหนึ่งใบกับ receipt ทั้งสอง, สร้างและ post AP invoice รวมหนึ่งใบ, อนุมัติและ export supplier batch, จับคู่ bank evidence ยอด THB 250,000 และตรวจ invoice paid, stock quantity/value, balanced journals กับ progress 6/6 จาก source recordsเดิม.

ผลทดสอบ `tests/purchase-to-payment.test.ts` ผ่าน 1/1 และ `tests/supplier-bill-matching.test.ts` ผ่าน 8/8 บน disposable PostgreSQL simulation. กรณีเฉพาะครอบคลุมการแข่งขันจับคู่ receipt, การคืน AP draft, การปล่อย receipt ทั้งสองกลับมาแก้ไขแบบ atomic และ Company policy ที่บังคับ matching พร้อม version/idempotency/immutable history. Browser acceptance ผ่านทั้ง Desktop Chrome และ Pixel 7 โดยเปิด required, ตรวจคำอธิบาย control, คืนค่า optional และตรวจประวัติผ่าน UI จริง. บันทึกผลและ scope แบบ checksum ใน evidence ledger แล้ว. Partial/split requisition allocation และ UOM conversion ปิดด้วย migration 0238 และ owner-service acceptance แล้ว; multi-role purchase-to-payment browser, provider, production และ business sign-off ยังคงค้าง.

ผู้ใช้ยืนยันให้ใช้ข้อมูลจำลองก่อน ยังไม่พร้อมระบุบริษัท/สาขา pilot ผู้รับรอง Finance/Payroll/Operations หรือ public production hostname จึงเก็บ production acceptance เป็น pending และคง `https://erpx.jesadakorn.com` เป็น development บน 3100 ส่วน production 3101 ไม่เปลี่ยน

## ฐานอ้างอิง

- [รายงานสถานการณ์ธุรกิจ](cross-department-business-scenarios.md): baseline 21 scenarios / G01–G20; ผลในรายงานเป็น snapshot ก่อนการแก้รอบนี้
- [ทะเบียนความสามารถ](../status/capability-delivery-matrix.md): ปรับ generator รวม Legal เป็น 183 capabilities
- [Acceptance manifest](../status/business-acceptance-manifest.json): เชื่อม capability กับ scenario, phase, เจ้าของงาน, แหล่งโค้ด/ทดสอบ และ gate แยก automated/browser/provider/production
- Manifest เป็นทะเบียนตั้งต้น ทุก gate ยัง pending; test path และสถานะเดิมไม่ใช่การรับรองราย capability เก็บ observation แยกใน `docs/status/business-acceptance-evidence.json` พร้อม durable evidence/checksum; generator ตรวจ ID, scope, environment และ checksum ก่อนสร้าง manifest และไม่ยกระดับ gate อัตโนมัติ ยังต้องมี workflow ผู้รับรองจริงก่อน production sign-off

## ช่วง 0 ที่เปลี่ยนแล้ว

| งาน | สิ่งที่เปลี่ยน | ข้อจำกัดที่ยังเหลือ |
| --- | --- | --- |
| G19 test runner | `scripts/local/test.ts` ตรวจ Bun 1.2.17 / Node 24.20.0 โหลดเฉพาะ test administrator URL จากไฟล์ environment และเรียก Vitest ด้วย pinned Node; แก้ runbook port ทดสอบเป็น 5432 ตาม isolation guard | ผล provider tests ที่ skip ไม่ถือว่าผ่าน; ผล full regression รอบใหม่อยู่ในหลักฐานช่วง 1 |
| G13 POS currency | ส่ง currency แยกจาก accounting configuration ให้แคชเชียร์; แสดงเอกสารด้วย sale currency; refund/report/payment mix ใช้ currency ของรายการ และรายงานแยกกลุ่ม currency | ยังใช้ amount contract สองตำแหน่งเดิม; ไม่ได้เพิ่ม multi-currency accounting หรือเปลี่ยนยอดประวัติ; อุปกรณ์และ receipt จริงยังต้องรับรอง |
| G20 routing | เพิ่ม Work → Routing attention โดยใช้ source-authorized `flow.canManage`, current step และ blocker; เปิด record เดิมเพื่อ reassign พร้อมเหตุผล | ไม่ใช่ global scan ทุก record; ยังขาดเวลาเริ่มติด routing, escalation SLA, alert และ acceptance หลายบทบาทผ่าน browser; ไม่ได้เปลี่ยนผู้รับของรายการใน dev |
| Manifest | รวม Legal และแยก gate จาก implementation; ทดสอบ uniqueness 183 capabilities และ coverage 21 scenarios | เจ้าของเป็นบทบาท ยังไม่มีผู้รับรองจริง; mapping ระดับ module ยังต้องละเอียดถึง acceptance case ของแต่ละ capability |

## หลักฐานช่วง 0

- ชุดแรก capability/work views/POS ผ่าน 31 tests; หลังเพิ่มกรณี historical currency รัน POS + capability ใหม่ผ่าน 26 tests (33.76 วินาที) และ flow routing ผ่าน 11 tests (32.16 วินาที) รวมผลล่าสุดจาก 4 suites **43 tests passed, 0 skipped** บนฐานข้อมูลทดสอบแยก
- Routing simulation: ถอนสิทธิ์ reviewer เดิม → ปฏิเสธผู้ไม่มีสิทธิ์/ผู้ขอเอง → reassign ให้ reviewer ที่มีสิทธิ์อยู่ใน approval pool → เก็บประวัติ approval เดิม → เตรียมบัญชีและอนุมัติต่อได้; เป็น automated acceptance ยังไม่ใช่ browser หลายบทบาท
- `bun run typecheck`: ผ่าน
- `bun run lint`: ไม่มี error; พบ warning เดิม 3 จุดใน settings, service-navigation และ PMS หลังลบ unused currency import ที่แก้รอบนี้
- `bun run deploy:build`: ผ่าน isolated release `.runtime/releases/2026-09-19T17-26-41-556Z`; ไม่ได้ deploy release นี้ไป production
- Browser authenticated holding owner ที่ public dev: POS catalog แสดง USD ตาม company; ยังแจ้ง Not configured และไม่มี cash session จึงไม่อ้างว่าขายจบแล้ว
- Browser Work `?view=routing`: แสดง 37 matching items จากข้อมูลปัจจุบัน รวม expense ที่ primary ไม่ eligible พร้อมลิงก์กลับ owner record; จำนวนนี้ไม่ใช่จำนวนงานทั้งระบบ
- `curl` local/public `/api/health`: HTTP 200 ทั้งคู่; dev listener PID 56148, cwd repository, reuse process เดิม
- Log ตรวจอยู่ใต้ ignored `.scratch/cross-department-audit/phase0-*.log`; ไม่ใช่หลักฐาน sign-off ที่เก็บถาวร

## ช่วง 1 — คืนสินค้าที่จ่ายแล้วและการคืนเงินจริงตามหลักฐานธนาคาร (จำลอง)

รายละเอียด flow / API / runbook / gates อยู่ใน [S04 refund simulation](return-refund-simulation.md)

- เพิ่ม migration `0203_return_refund_obligations.sql` แบบ additive: ไม่เปลี่ยนยอดเดิม ไม่สร้าง approval ย้อนหลัง
- `src/server/refunds.ts` เป็น owner ของ obligation/instruction; source services ของ Commerce/Finance ยังคงเป็นเจ้าของ invoice, goods และ journal
- เพิ่ม API และหน้า Finance → Return refunds พร้อม Work/Inbox/Calendar; approval แยกคนกับ Treasury, export มี checksum/receipt ที่เรียกคืนได้, unknown ไม่ปล่อยยอดให้จ่ายซ้ำ
- Bank confirmation บันทึก payment/GL/ยอดคงเหลือ/bank match พร้อมกัน; ป้องกัน amount/currency/account/date mismatch, over-refund, duplicate source/key, stale version, closed period และการจองยอดพร้อมกัน
- เพิ่ม close guards สำหรับ return adjustments ที่ยังไม่ complete กับ refund export ที่ยังไม่รู้ผล และตรวจ draft/bank อีกครั้งตอน final close
- ใช้บัญชี journal สต็อกต้นทางในการกลับรายการ แม้ config ปัจจุบันเปลี่ยนแล้ว
- แก้ปุ่ม approve/reject return ให้ส่ง action จาก submit button ได้ถูกต้อง
- Apply migration เฉพาะ dev `erpx_dev_*` บน 5432; production service 3101 และ Tunnel ไม่เปลี่ยน

ผลตรวจระหว่างพัฒนา:

- Return/Finance correction/Work cache regression: 16 tests passed
- Handoff + Calendar/API/Work views: 25 tests passed
- Return + period-close/Finance correction: 5 tests passed ก่อนเพิ่ม tax case
- Return suite ล่าสุด: 5 tests passed รวม fully-paid gross tax refund และกลับบัญชีตาม journal เดิม
- Migration repeatability: 1 test passed หลังปรับจำนวนตาราง expected จาก 320 เป็น 322 และเพิ่มการตรวจว่าตารางใหม่ว่าง
- Browser session เดิมบน public dev: หน้า Return refunds และ Calendar layer Refund instructions เปิดได้; Company ปัจจุบันไม่มี refund fixture จึงยังไม่อ้างว่าผ่าน approve/export/confirm ผ่าน browser หลายบทบาท
- Typecheck และ lint ผ่าน (warning เดิม 3 จุด); final isolated release build ผ่านที่ `.runtime/releases/2026-09-19T18-06-21-315Z` รวม Calendar/notifications/close guards แล้ว โดยไม่ deploy production
- Full regression เริ่ม 00:52:43 Bangkok: 80 files, 77 passed / 1 failed / 2 skipped; 727 tests passed / 1 failed / 3 skipped ใน 815.67 วินาที ความล้มเหลวเดียวคือ migration fixture คาด 320 tables แทน 322 ซึ่งแก้และรันไฟล์นั้นซ้ำผ่านแล้ว ไม่อ้างว่ารอบ full เดิมผ่านทั้งหมด
- หลังเพิ่ม/ปรับ tax/source-account/close cases รัน return suite ล่าสุด 5 tests ผ่านใน 34.70 วินาที; การเพิ่ม Inbox/Calendar ตรวจแยก 25 tests ผ่านตามข้างต้น ยังไม่ถือเป็น browser หลายบทบาทหรือ provider acceptance

## งานในแผนที่ยังไม่ปิด

| ช่วง | งานหลัก | สถานะ |
| --- | --- | --- |
| 0 | routing recovery acceptance, escalation และ workflow ผู้รับรองจริง | ได้บางส่วนตามตารางข้างต้น |
| 1 | paid returns/refund obligation, independent approval, payment export/batch, reconciliation, FX, period close และ intercompany | เพิ่ม paid customer/supplier returns → refund obligation → partial instruction/export → bank/GL reconciliation แล้ว; เพิ่ม AP supplier batch แบบผูก invoice และ bank/GL แล้ว; Payroll/shared batch, FX และ intercompany ยังไม่ครบ |
| 2 | PR/RFQ, shared reservation, transit, lot/serial/quality, delivery/POD | ยังไม่ปิดช่องว่าง G02/G03/G12 |
| 3 | customer identity/authority, CRM, Support SLA, Portal/Email, Marketing handoff | ยังไม่ปิด G01/G02/G09/G18 |
| 4 | project milestones/customer acceptance, assets/field service, Legal handoff | ยังไม่ปิด G05/G09/G11/S21 |
| 5 | POS room-charge, PMS online payment, SiteMinder, E-commerce | ยังไม่ปิด G06/G07/G08/G15 |
| 6 | BOM และ formula/batch, planning, WIP/cost/genealogy | ยังไม่ปิด G03/G10 |
| 7 | People handoff, recruitment/benefits/learning, Thai payroll pack/shadow runs | ยังไม่ปิด G11/G16; statutory pack ต้องตรวจแหล่งทางการและรับรองก่อนจ่ายจริง |
| 8 | Platform/Billing/Brand/Collaboration/AI/Executive | ยังไม่ปิด G14/G17/G18 และ capability ที่เหลือ |
| เปิดใช้ | master/open document migration, business UAT, provider/device acceptance, restore drill, production sign-off | pending ตามคำตอบผู้ใช้; จำลองได้ก่อน แต่ห้ามนับเป็นผ่านจริง |

การพัฒนาถัดไปใช้ transaction/idempotency/version/independent approval ของ owner services เดิม รักษาเอกสารและยอดเก่า ไม่ใช้ reverse receipt แทนเงินคืน และไม่เปลี่ยน Tunnel หรือย้ายข้อมูลจำลองเป็นธุรกรรมจริง

## ทะเบียนหลักฐานที่ไม่ถูก generator เขียนทับ

- เพิ่ม `docs/status/business-acceptance-evidence.json` และ validator `src/data/acceptance-evidence.ts`; generator อ่านอย่างเดียว เชื่อม evidenceRecordIds ต่อ capability และตรวจ SHA-256 ของไฟล์หลักฐานก่อนเขียนผล
- บันทึกผล S04 ล่าสุดไว้ใน `docs/testing/evidence/2026-09-20-refund-tests.txt` แยกจาก scratch; เป็น observation บางส่วน ไม่ใช่ sign-off ครบ capability
- แก้ observation ด้วยการเพิ่ม record ใหม่ที่อ้าง `supersedes`; รักษา record/ไฟล์เดิมผ่าน Git review ทะเบียนไฟล์นี้ยังไม่ใช่ signed audit store และไม่อ้างว่าป้องกันผู้มีสิทธิ์แก้ repository ได้
- Simulation ใช้รับรอง provider/production ไม่ได้; ข้อมูลผิด capability/scenario, duplicate ID, supersession ผิด scope, path นอกที่เก็บถาวร หรือ checksum ไม่ตรงจะทำให้ generation ล้มเหลว
- ทุก gate ยังคง pending จนมีการรับรองเต็มขอบเขตโดยเจ้าของงานที่เหมาะสม

ตรวจทะเบียนหลักฐาน: acceptance-evidence + capability-delivery รวม 8 tests ผ่าน; typecheck ผ่าน และ ESLint ของไฟล์ที่เพิ่ม/แก้ในทะเบียนไม่มี error/warning

## ช่วง 1 — Supplier payment batches (เพิ่มต่อ)

รายละเอียดและ runbook: [Supplier payment simulation](supplier-payment-simulation.md)

- Additive migration `0204_supplier_payment_batches.sql` เพิ่ม batch/instruction และ work cache invalidation; apply เฉพาะ dev แล้ว รวม 324 tables
- Owner service/API/UI พร้อม Finance navigation, Work/Inbox/Calendar; กัน invoice balance ตั้งแต่เตรียม, independent approval, Treasury คนที่สาม, CSV checksum/receipt และผลธนาคารรายรายการ
- ยืนยันสำเร็จแล้ว payment/allocation/bank/GL/invoice commit พร้อมกัน; unknown ไม่คืนยอด, failed ต้องผู้ตรวจอิสระ, cancel ได้เฉพาะก่อน export
- กันการชำระเดิม/คืนสินค้าและ contractor batch ใน People ไม่ให้ใช้ invoice เดียวกันข้าม owner
- Regression ที่เกี่ยวข้อง 30 tests ผ่าน; เพิ่ม source-return/settlement guards แล้ว 7 tests ผ่าน; ชุดล่าสุด supplier/payroll/work cache/refresh 20 tests ผ่าน ไม่มี skip ในชุดเหล่านี้
- Typecheck ผ่าน; lint ไม่มี error (3 warnings เดิม); browser list/form/checkbox/configured bank และ Calendar layer บน public dev ผ่านการตรวจ แต่ยังไม่ได้ submit approval/export/confirm หลายบทบาทผ่าน browser
- Source-linked AP subset นี้ยังไม่เท่ากับปิดช่วง 1: FX, intercompany, unified Payroll/refund batch, migration/UAT และ production sign-off ยังเหลือ

หลักฐานสุดท้ายของ supplier batch:

- Supplier suite ล่าสุด 4 tests passed รวม failed → เตรียมใหม่ → bank confirm → invoice paid → Work completed; ไม่ย้าย failed ที่ยังมีหนี้ไป completed
- Navigation + counts 52 tests passed; manifest/evidence 8 tests passed
- Isolated release build ผ่าน `.runtime/releases/2026-09-19T18-45-07-920Z`; ไม่ deploy production
- Local/public `/api/health` HTTP 200; dev listener PID 56148 cwd repository ยังคงทำงานบน 3100
- หลักฐานถาวรล่าสุด `docs/testing/evidence/2026-09-20-supplier-completion-tests.txt` เพิ่ม record แบบ supersedes โดยเก็บผลก่อนหน้าไว้

## ชุด A — Company routing review (เพิ่มต่อ)

- เพิ่ม scan/review UI และ GET API สำหรับ 11 named Flow sources แยกจากรายการ Work พร้อม source access และ Company management guard; คืนเฉพาะ blockers ที่ผู้ตรวจมีสิทธิ์
- เอา cutoff ล่าสุด 250 รายการจากตัวรวบรวม order/invoice/journal/payroll Work ออก เพื่อไม่ให้งานเก่าถูกละทิ้ง
- ใช้ reassign command/FlowPanel เดิม ตรวจ version/eligibility/reason และคง completed decisions
- Routing integration 14 tests ผ่าน รวมงานเก่าท่ามกลาง owner-created drafts ใหม่ 251 รายการ; Work/cache/refresh 21 tests ผ่าน
- Browser ใหม่ยังตรวจหลัง login ไม่ได้: session เดิมหมดอายุ หน้า public dev redirect ไป sign-in; ไม่ถือว่า browser gate ผ่าน
- ยังเหลือ owner-specific routing adapters, background scan/first-blocked timestamp, business-calendar/SLA/escalation และ per-capability acceptance cases; ชุด A ยังไม่จบ
- รายละเอียด: [Company routing review](routing-attention-simulation.md)

ตรวจปิดรอบ routing: typecheck ผ่าน, ESLint ไฟล์ใหม่ผ่าน; isolated build ผ่าน `.runtime/releases/2026-09-20T02-57-40-091Z`; local/public health HTTP 200 และ dev เดิมยังทำงาน ไม่ deploy production

## แผน A–J — งานเพิ่มวันที่ 20 กันยายน 2026

รอบนี้ยังไม่ปิด A–J ทั้งแผน งานโค้ดที่ขาดยังเป็น implementation gaps ไม่ถูกย้ายไปหมวด provider/UAT เพื่อให้ดูเสร็จ

### C: Sales/POS stock reservations

- เพิ่ม [shared stock reservation](shared-stock-reservation-simulation.md) พร้อม additive migration 0205, owner command/API/UI, audit/receipt/outbox, current permission และ source version.
- ยอดจองจาก Sales/POS ถูกรวมใน availability และ DB guard ของทุก stock writer; partial delivery consume เฉพาะบรรทัดต้นทาง; release/cancel/expiry ไม่แก้จำนวนหรือต้นทุนสินค้าเดิม.
- Owner regression สุดท้าย 4 suites / 32 tests passed รวม Sales แย่งชิ้นสุดท้ายกับ POS. Typecheck/lint ผ่าน (3 warnings เดิม). Isolated release `.runtime/releases/2026-09-20T03-13-20-715Z` ผ่านสำหรับ reservation change; ไม่ deploy production.
- Apply เฉพาะ dev; 325 base tables. Browser authenticated บน public dev อ่าน Sales list/เอกสารเก่าได้ แต่ไม่มี approved open order สำหรับตรวจฟอร์มจองครบหลายบทบาทใน session นี้.
- เปลี่ยน registry ของ `inventory/reservations` จาก deferred เป็น implemented บางส่วนตามกติกาทะเบียน; จำนวน deferred เหลือ 84 แต่ทุก acceptance gate ยังคง pending. ไม่ถือจำนวนนี้เป็นเปอร์เซ็นต์ความพร้อมใช้งาน.

### A/J: Business readiness dashboard

- หน้า `/settings/business-readiness` และ GET `/api/v1/holdings/{holdingSlug}/business-readiness` ให้ Company organization manager ตรวจครบ 183 capabilities / 21 scenarios พร้อม owner, package, G01–G20, implementation gaps และ independent gates.
- แสดง scoped observations ล่าสุด โดยซ่อน record ที่ถูก supersede แต่ไม่ลบทิ้งจากทะเบียน. ผล passed บางส่วนไม่เปลี่ยน gate และไม่รับรอง Company ที่เปิดดู.
- Snapshot มาจาก generator ที่ตรวจ SHA-256 ของไฟล์หลักฐาน; isolated release build ตรวจ freshness ก่อน copy source. Snapshot หรือหลักฐานไม่ตรงจะหยุด build.
- Tests 10 passed: snapshot/manifest/ledger ตรงกัน, no gate promotion, supersession และ current Company authority/revocation/scope.
- Browser public dev แสดง 183 capabilities, 21 scenarios, 732 gates pending; filter C แสดง 14 capabilities และ reservation observation เป็น partial. นี่คือ browser acceptance ของ dashboard เท่านั้น.

### สถานะขอบเขตทั้งหมดหลังงานรอบนี้

| ชุด | ส่วนที่มีหลักฐานเพิ่ม | งานหลักที่ยังไม่ปิด |
| --- | --- | --- |
| A | ทะเบียน183, scoped evidence, routing review, dashboard | acceptance cases รายพฤติกรรมทุก capability, non-Flow routing adapters, SLA/calendar/escalation |
| B | refund/AP batch และ bank/GL reconciliation จำลอง | shared Payroll adapter, FX, opening reconciliation, schedules, compensation, intercompany/consolidation |
| C | Sales/POS reservation และ partial consume/release/expiry | PR/RFQ/supplier promises/variance, transit, QC/lot/serial/UOM/landed cost, shipment/POD, adapters อื่น |
| D | ไม่มี owner flow ใหม่ในรอบนี้ | identity/merge/authority, CRM, Support SLA, Portal/Email, Marketing |
| E | ไม่มี owner flow ใหม่ในรอบนี้ | Projects/change/acceptance/billing, Field Service, Assets และ Legal handoff ที่เหลือ |
| F | POS availability ใช้ยอดจองร่วมกับ Sales | room charge, PMS online payment, SiteMinder, E-commerce และอุปกรณ์/browser acceptance |
| G | ยังไม่มี production adapter | Planning, BOM/formula, production lifecycle, WIP/cost, genealogy/recall |
| H | contractor batch ป้องกันชน AP จากรอบก่อน | People/Talent/Benefits/Learning handoffs และ statutory Thai Payroll/adapter/shadow runs |
| I | capability gaps แสดงผ่าน readiness dashboard | domain capabilities ที่ยัง deferred/partial ตามทะเบียน, provider และ operational recovery |
| J | dashboard และหลักฐาน tests แบบ checksum | S01–S21 ครบหลายบทบาท, migration rehearsal, restore drill, runbooks/UAT และ release candidate sign-off |

### J: isolated logical restore rehearsal

- เพิ่ม [recovery simulation](recovery-simulation.md) ที่ใช้ owner commands สร้าง Sales → reservation → partial delivery แล้ว dump/restore ไปฐานทดสอบใหม่.
- ผลล่าสุดเทียบครบ 325 base tables ด้วย row counts และ SHA-256; order/stock/reservation ตรงต้นทาง และ stock guard ยังปฏิเสธการใช้ของที่ถูกจองหลัง restore.
- สร้าง/restore/ตรวจฐานปลายทางใช้ 8,687 ms บน fixture ขนาดเล็ก; archive และฐานปลายทางชั่วคราวลบหลังตรวจ โดยตรวจชื่อและ ownership ก่อน.
- เป็น partial observation ของ backup recovery เท่านั้น. RPO ≤15 นาที / RTO ≤4 ชั่วโมงของ production ยัง pending; ยังไม่รวมไฟล์, control/read databases, WAL/PITR, remote backup, jobs และ external receipts.
- ทะเบียน evidence เก็บ source IDs, checksum, revision และ hashes ของ source files ที่เกี่ยวข้อง โดยไม่บันทึก credential.

การแก้ขอบเขต expiry เพิ่มเติม: release เลือกเฉพาะยอดจองที่ยังไม่หมดอายุ เพื่อไม่ให้แถวประวัติที่หมดอายุไปรับยอด release แทน reservation ปัจจุบัน. รัน stock + navigation หลังแก้ 53 tests ผ่าน; เก็บ observation ใหม่ supersede ผลก่อนหน้าโดยคงหลักฐานเดิม.

### ผล regression รวมหลังเพิ่ม reservation / readiness / restore rehearsal

- รอบแรกที่เริ่มขณะยังแก้โค้ดหยุดโดยเจตนาและไม่นับเป็นหลักฐานปิดงาน.
- รอบเริ่ม 10:29:33 Bangkok ใช้ 1000.11 วินาที: 86 files — 83 passed, 1 failed, 2 skipped; 750 tests — **746 passed, 1 failed, 3 skipped**.
- Failure เดียวคือ widget `commerce-orders-detail-stock-reservations` ยังไม่ได้ลงทะเบียน. เพิ่มใน `src/data/feature-widgets.json` แล้ว; รัน feature-widget-registry + page-layouts ซ้ำผ่าน **10 tests**. ไม่ได้รัน full suite ใหม่หลังการแก้ทะเบียน จึงไม่เปลี่ยนผล full run เดิมเป็น passed.
- Stock expiry/consume/concurrency ที่เคยล้มในรอบคาบเกี่ยวผ่านทั้ง 3 cases ในรอบรวมนี้แล้ว. Readiness manifest/scope tests และ recovery rehearsal รวมอยู่ในรอบนี้ด้วย.
- หลักฐานและ checksums: [regression summary](evidence/2026-09-20-aj-regression-summary.json), [full run](evidence/2026-09-20-aj-regression.txt), [widget correction](evidence/2026-09-20-widget-correction-tests.txt).
- Typecheck ผ่าน; lint ไม่มี error (3 warnings เดิม). ยังไม่ประกาศ release candidate ว่าครบ S01–S21 หรือปิด A–J.

Isolated release หลังแก้ widget registry ผ่าน: `.runtime/releases/2026-09-20T03-46-14-900Z` (ไม่ deploy). ตรวจ local/public `/api/health` ได้ HTTP 200 ทั้งคู่; dev PID 56148 ยังเป็น workspace นี้บน 3100. หน้า readiness ผ่าน authenticated browser ทั้ง filter และเปิดรายละเอียด evidence; production 3101 และ Tunnel ไม่เปลี่ยน.


## ต่อชุด C — Quality quarantine (2026-09-20)

**ปิดเฉพาะส่วนพัฒนานี้:** whole product/warehouse hold → independent pass/fail inspection → third-person release; ผลตรวจ immutable, hold ซ้อนกัน, current permissions/version/idempotency, Work/Inbox/Calendar, DB stock/reservation guards และ POS/Commerce availability. ผู้ตรวจไม่มีสิทธิ์ปล่อยผลตรวจของตนเอง ผู้สร้างก็อนุมัติปล่อยเองไม่ได้. ไม่มีการเปลี่ยนยอดหรือประวัติสินค้าเดิมเพื่อให้ผ่าน test.

**ผลตรวจ:** owner regression 7 files / 116 tests ผ่าน, ไม่มี skipped; หลังเพิ่ม quality flag ใน paginated Commerce stock รัน Quality อีก 5 tests ผ่าน. Typecheck ผ่าน; lint 0 errors / 3 warnings เดิม. รอบก่อนหน้าพบ navigation expectations ยังไม่รวม Quality และ assertion ของ test อ่านคอลัมน์ใบเสร็จผิด แก้แล้วรันชุดที่เกี่ยวข้องใหม่ผ่าน.

**Browser:** เปิดหน้าคิว Quality ด้วย session holding owner ที่ได้รับอนุญาตผ่าน public dev ได้ เห็น selector, วันนัด, เหตุผล, คิวและ breadcrumb; ตรวจภาพหน้าจอแล้ว. ไม่ได้ส่งคำสั่งกักสินค้าบนข้อมูล dev และยังไม่มี browser acceptance หลายบทบาท.

**ยังไม่จบชุด C หรือ A–J:** lot/serial-level QC, measurements, private evidence attachments, scrap/supplier-return disposition, cancellation, timed escalation และ Company calendar/policy snapshots ยังต้องพัฒนา; pipeline PR/RFQ/partial receiving/transit/POD และชุด D–I ที่ระบุไว้ก่อนหน้ายังคงค้าง. Provider/production/UAT gates ไม่เปลี่ยนเป็นผ่าน.

คู่มือและเกณฑ์: [Quality hold simulation](quality-hold-simulation.md). หลักฐาน: [owner regression](evidence/2026-09-20-quality-tests.txt), [source context และ browser scope](evidence/2026-09-20-quality-context.json), [stock projection regression](evidence/2026-09-20-quality-projection-tests.txt). Capability registry ให้ inspections/results/release-gates เป็น implementation แบบยังมี gap; ทุก acceptance gate ยังคง pending.

ตรวจเพิ่ม Work/cache/navigation/acceptance/readiness: รอบแรก 4 files ผ่าน, Work launcher 1 expectation ยังไม่มี Quality. แก้ expectation แล้วรัน Work views ซ้ำผ่าน 6 tests; ไม่แปลงผลรอบรวมก่อนหน้าเป็น passed. Final Quality projection เพิ่ม assertion ว่า selector ไม่ส่งต้นทุนและ stock page แสดง hold ผ่าน 5 tests. [ผลตรวจรวมรอบนี้](evidence/2026-09-20-quality-verification-summary.json).

Isolated release ล่าสุด `.runtime/releases/2026-09-20T04-25-38-947Z` ผ่าน ไม่ deploy. Dev PID 56148 ยังอยู่ใน workspace บน 3100; local/public health HTTP 200 และหน้า Quality ผ่าน browser หลัง reload. ไม่เปลี่ยน production 3101 หรือ Tunnel. Full repository regression ยังไม่ได้รันใหม่หลังเพิ่ม Quality; ไม่ประกาศปิดชุด J.


## ต่อชุด C — Dispatch / transit / partial receiving (2026-09-20)

เพิ่ม transport mode ให้เอกสารโอนใหม่ โดยคง direct mode และประวัติเดิม. Approval สร้าง transport ยังไม่ย้าย stock; source dispatch จึงลดต้นทางและตั้งสินค้าระหว่างทาง. ปลายทางรับบางส่วนได้ด้วยคนละคนกับผู้ส่ง/ผู้เตรียม. รายงานของขาดกันจำนวนรอตรวจไว้แต่ไม่ตัด asset จน Finance อิสระรับรองและลง GL ในงวดเปิด. เก็บ immutable movements/decisions และ cumulative rounding ให้ต้นทุนรวมไม่เปลี่ยน. Work/Inbox/Calendar และ stock valuation อ้าง source เดียวกัน.

Owner cases 7 ผ่าน รวม direct compatibility, partial/race/replay, quarantine, closed period, loss rejection/approval/GL และ scope. Adjacent regression รอบ 6 files มี 5 passed / 1 failed (navigation expectation), 97 passed / 1 failed; แก้ทะเบียน expectation แล้วรัน navigation ซ้ำผ่าน 50 tests. ไม่เปลี่ยนผลรอบรวมเดิมเป็น passed. Typecheck ผ่าน; lint 0 errors / 3 warnings เดิม.

คู่มือ/ข้อจำกัด: [Inventory transit simulation](inventory-transit-simulation.md). หลักฐาน [source context](evidence/2026-09-20-transit-context.json). Migration 0207 ใช้กับ dev แล้ว ไม่เปลี่ยน production/Tunnel. หน้า public transport queue เปิดผ่าน session เดิมได้; กำลังเตรียม browser simulation หลายบทบาทบนฐานข้อมูลแยก. ยังไม่ปิด C/S14 ทั้งหมดหรือ A–J.


## Browser จำลองหลายบทบาท — 2026-09-20

- **ปิดแล้วในขอบเขตนี้:** สร้าง environment แยกบน loopback 3102 ด้วยบัญชีจำลองห้าคน login ปกติ; คลังต้นทาง → ผู้รับอนุมัติ → dispatch → partial receipt → Finance loss review; คลัง → Quality inspector → Quality manager release โดยผู้รับทุกคนเปิดงานจาก Work ของตนเอง
- จำนวน 3 = รับ2 + loss1; มูลค่า THB10.00 = stock6.67 + loss3.33; ระหว่างกัก stock คง2/value6.67 แต่ available0 และกลับเป็น2หลังผู้อนุมัติคนที่สาม release
- แก้ source transfer แสดง transport status/ทีมรับช่วงจาก owner projection เดียวกับ Work และเพิ่ม Back to Work ให้ transport detail; แก้ Commerce permission error ให้แสดง ERPX_PERMISSION แทน service-unavailable โดยทดสอบจริงด้วย quality-only role
- Owner regression หลังแก้ projection: 2 files / 12 tests ผ่าน; typecheck ผ่าน; lint 0 errors / 3 warnings เดิม Full regression รอบใหม่กำลังรัน ไม่ยกผล scoped tests เป็นผลทั้งระบบ
- หลักฐาน browser และ source rows อยู่ที่ `evidence/2026-09-20-transit-quality-browser.json` และ `evidence/2026-09-20-transit-quality-browser-records.json` พร้อม checksum ใน ledger เป็น partial observation ไม่ปลด gate ทั้ง capability
- **ข้อจำกัดที่พบใน fixture:** inventory account ใช้ asset เดิมชื่อ cash และ seed opening stock โดยไม่มี opening GL จึงไม่ใช่หลักฐานเปิดงวด/ย้ายข้อมูลครบ; ต้องใช้ผังบัญชีและ opening reconciliation จำลองแยกในการตรวจส่วนนั้น
- **ยังเหลือ:** RFQ/เปรียบเทียบผู้ขาย/supplier promise/variance matching, lot/serial/UOM/landed cost, customer shipment/POD และชุดอื่น A–J ตามรายการเดิม; provider/UAT/production ยัง pending ตามขอบเขตผู้ใช้

## ต่อชุด C — Supplier delivery confirmations (2026-09-20)

เพิ่ม immutable supplier commitments บน PO ที่อนุมัติแล้ว แยกจำนวน/วันส่งได้ มี unconfirmed shortfall, replacement/withdrawal และหักยอดรับจริงตามลำดับวันส่งโดยไม่หัก receipt ก่อน snapshot ซ้ำ. Work/Inbox/Calendar อ่าน source เดียวกันพร้อม Company/Branch/Department guards. ไม่แก้ required date เดิม ไม่สร้าง stock/GL จากคำยืนยันผู้ขาย.

- Owner regression 7 files / 33 tests ผ่าน รวม migrations ครบและ idempotence; รอบแรก migration timeout ขณะ full regression รันพร้อมกัน แยกทวนและเพิ่มเวลาเฉพาะ migration acceptance แล้วผ่าน.
- Typecheck ผ่าน; lint 0 errors / 3 warnings เดิม; isolated release `.runtime/releases/2026-09-20T05-39-46-251Z` ผ่าน ไม่ deploy.
- Full baseline รอบก่อนเริ่ม supplier work: 88 files / 86 passed / 2 skipped, 760 tests passed / 3 skipped. Supplier work ไม่อยู่ใน initial discovery จึงใช้ owner regression แยก ไม่อ้างว่า full run ครอบคลุม revision ใหม่.
- Browser หลายบทบาทกำลังตรวจบนฐานข้อมูลแยกที่มี inventory/opening equity accounts และ opening GL ผ่าน independent approval; ไม่ยกระดับ gate ก่อนมีผลจริง.
- ยังเหลือ RFQ/เปรียบเทียบราคา, matching/variance, supplier Portal/Email, customer delivery promise, lot/serial/UOM/landed cost และ A–J ส่วนอื่นตามรายการเดิม.

หลักฐาน: [33 owner tests](evidence/2026-09-20-supplier-promises-tests.txt), [full baseline](evidence/2026-09-20-full-regression.txt); คู่มือ [supplier delivery confirmations](supplier-delivery-promises.md).

Supplier browser observation เสร็จในขอบเขตสี่บัญชีจริงบนฐานข้อมูลจำลอง: creator → independent purchasing approval from Work → split supplier commitments/Calendar → warehouse partial receipt from Work → replacement/withdrawal → independent Finance posting from Work. หลังรับ1จาก3 คงค้าง2; replacement2ไม่หัก receipt เก่าซ้ำ; withdrawal คืน unconfirmed2 พร้อมประวัติครบ. Receipt stock THB10 ตรง journal Dr inventory10/Cr payable10 posted โดย Finance. พบ layout ฟอร์มอ่านยากและแก้ scoped CSS พร้อมตรวจ screenshot ซ้ำ. หลักฐาน [browser](evidence/2026-09-20-supplier-promises-browser.json) และ [source records](evidence/2026-09-20-supplier-promises-browser-records.json). ยังเป็น partial observation; PO คงยอดค้างที่ต้องติดตาม ไม่ใช่ปิด S03 ทั้งหมด.

## ต่อชุด C — RFQ / comparison / independent award / requisition handoff

เพิ่ม demand → immutable offers → current supplier comparison → award proposal → independent review → draft PO ใน transaction เดียว; quote ที่ถูกแทนหรือหมดอายุเลือกไม่ได้ และแก้จำนวน/ราคา/ภาษีที่ผูก award แล้วไม่ได้ด้วยการละ source ID. PO ยังคงต้องผ่าน approval เดิม. มี service/API/UI, Work/Inbox/Calendar และ current scope/permission/version/idempotency guards; ถ้าไม่มี reviewer แสดงเหตุผล blocked.

เพิ่ม source-linked approved PR → RFQ: ผู้รับคำขอที่เริ่มงานแล้วจับคู่ทุก item กับ product/quantity เดิม เก็บ request/evidence version และ item snapshot; มี RFQ active ได้หนึ่งรายการต่อคำขอ; ห้ามสร้างหรือ link PO ข้าม active award. เมื่อสร้าง PO ให้ PR/RFQ/PO ผูกกันพร้อมกัน; comment ใช้ current request version โดยคง approved evidence. คำขอที่รอข้อมูลใหม่หรือ source evidence เปลี่ยนจะกั้น award จนแก้ต้นทาง.

- RFQ owner regression ก่อนเพิ่ม PR mapping: 5 files / 29 tests ผ่าน รวม Work/cache/Calendar/Commerce.
- PR mapping รอบแรก: 3 files / 27 tests ผ่าน รวม complete/idempotent migration337tables. หลังเพิ่ม source-waiting projection ทวน RFQ/employee 2 files / 26 tests ผ่าน.
- Typecheck ผ่าน; lint 0 errors / 3 warningsเดิม. RFQ releaseก่อน PR mapping `.runtime/releases/2026-09-20T06-11-00-649Z` ผ่าน; releaseหลังเพิ่ม PR `.runtime/releases/2026-09-20T06-22-57-097Z` ผ่าน ไม่ deploy.
- **Browser RFQ ยังยืนยันไม่ได้:** Chrome connection timeout ทั้ง navigation/focus และการผูกแท็บเดิม; ไม่มีการ bypass authentication. Provider/production gates ยัง pending. Supplier promise browser สี่บทบาทก่อนหน้านี้มีหลักฐานแยกและไม่ถูกนับเป็น RFQ browser acceptance.
- Partial/split requisition allocation และ UOM conversion ส่งมอบแล้วในรอบ 0238; ยังเหลือ supplier invitations/portal, browser หลายบทบาท, lot/serial/landed cost และชุด A–J ส่วนอื่น. ไม่ประกาศปิด C หรือทั้งแผน.

คู่มือและขอบเขต [RFQ simulation](procurement-rfq-simulation.md). Migrations0209–0210ใช้กับdevแบบadditive; production3101/Tunnelไม่เปลี่ยน.

## ต่อชุด C — split requisition / multi-award / UOM conversion

หนึ่งรายการคำขอสามารถแบ่งปริมาณไปหลาย RFQ, หลาย supplier award และหลาย PO ได้แล้ว โดยแต่ละ RFQ line เก็บ requested quantity/unit, product quantity/unit และอัตราแปลงแบบ immutable. PostgreSQL commitment counter ล็อกและบวกจำนวนแบบ atomic จึงมีผู้ชนะหนึ่งรายเมื่อจัดสรรพร้อมกัน และการยกเลิก RFQ คืนจำนวนให้ sourcing ใหม่. Legacy `purchase_order_id` คงเป็น pointer ใบแรกเพื่อ compatibility ขณะที่ `procurement_request_orders` เป็นความสัมพันธ์หลายใบจริง; progress รวมทุก PO/receipt/invoice/payment.

Owner-service acceptance ใช้ 24 each แบ่งเป็น 1 case ที่ 12 each/case และ 12 each จากผู้ขายอีกคน ตรวจ over-allocation, concurrent race, cancellation release, independent award และ PO สองใบ. Demo Company เพิ่มสินค้าหน่วย each/case, ผู้ขายสองราย และทางเลือก scenario ที่ reset ได้. Browser/provider/production/UAT gates ยังแยกและยังไม่ยกระดับ.


## ต่อชุด B/C — Supplier invoice matching และ purchase price variance

เพิ่ม migration0211 (340 tables), service/API/UI, immutable review evidence, quantity discrepancy/return/correction, independent Finance price review และ atomic approved review→AP→GL. Stock ยังคง original receipt cost; actual invoice difference ลง expense account ที่ผู้มีอำนาจเลือกพร้อมเหตุผล. Supplier partial returns แยก actual credit จาก stock cost และย้อน variance เดิม; GL preview ตรง posting owner.

- Automated owner รอบแรก 2 files / 4 tests ผ่านรวม migration340tables. Adjacent regression 32 passed / 1 failed จาก fixture มี Finance reviewer อีกคน จึงแก้ assertion ให้ถอน reviewer ที่เหลือด้วย; owner ทวน 4/4 ผ่าน. ไม่ซ่อน failed run.
- Typecheck ผ่าน; lint 0 errors / 3 warnings เดิม. Release `.runtime/releases/2026-09-20T06-45-44-335Z` ผ่านก่อนเพิ่ม return-after-approval; หลังแก้ return-after-approval ทวน 4 files / 14 tests ผ่าน และ release `.runtime/releases/2026-09-20T06-52-27-757Z` ผ่าน; readiness/evidence validation 6 tests ผ่าน.
- Public dev หน้า RFQ และ supplier-bills เปิดผ่าน authenticated browser แล้ว. Browser หลายบทบาทบน disposable run กำลังทดสอบ; ไม่ยกระดับ gate จากหน้า list อย่างเดียว.
- ยังเหลือ mandatory Company matching policy, multi-receipt/partial invoice, landed cost, FX และ A–J ส่วนอื่นตาม backlog. นโยบายลง price variance เป็น expense ต้องให้บริษัทรับรองก่อนใช้จริง.

คู่มือ [Supplier invoice matching](supplier-invoice-matching.md).


RFQ→PO→receipt→supplier matching→AP browser observation ผ่านในขอบเขตสี่บัญชีจริงบนฐานจำลองแยก: ผู้จัดทำ, purchasing manager, warehouse receiver และ Finance. ผู้รับเข้า Home/Work ของตนเอง; creator approve ตัวเองไม่ได้; ผู้ review ไม่ได้ปุ่ม AP หากไม่ใช่ named owner. Invoice THB25.68 (net24/tax1.68) เทียบ receipt stock20/variance4 และ posted GL ตรง preview. ยังไม่จ่าย: fixture ไม่มี Treasury จึงแสดง blocked owner และ outstanding25.68 ชัดเจน. ไม่ถือว่า S03/B/C จบทั้งชุด.

หลักฐาน [browser observation](evidence/2026-09-20-rfq-bill-browser.json), [source records](evidence/2026-09-20-rfq-bill-browser-records.json), [14 owner/adjacent tests](evidence/2026-09-20-supplier-bill-tests.txt). เพิ่ม guard คิว PO ระหว่าง matching review (15 routing/owner tests ผ่าน) และกั้น period close เมื่อมี supplier bill ยังไม่ post (4 owner tests ผ่าน). Invoice UI ซ่อน standalone-only void action สำหรับ source-linked invoices. ปิด correction หลังสร้าง AP draft ในระดับ automated แล้ว: independent reviewer ยกเลิกเฉพาะ draft และปล่อย receipt สำหรับฉบับแก้ใน transaction เดียว โดยเก็บประวัติและต้นทุนสินค้าเดิม; internal AP number แยกจาก supplier reference เพื่อใช้เลขบิลเดิมกับฉบับแก้ได้. Posting/correction race สำเร็จได้ทางเดียว. Matching/returns/payments regression 15 tests ผ่าน. เพิ่มคิวขอฉบับแก้ให้ named PO owner; ยังต้อง browser correction acceptance และงานที่ระบุไว้ในคู่มือ ก่อนนับ capability ทั้งหมดว่าครบ.


AP draft correction verification ล่าสุด: [owner 6 tests](evidence/2026-09-20-bill-correction-owner-final.txt) ผ่านรวมผู้รับงานถูกถอนสิทธิ์และ posting/correction race; [routing regression 17 tests](evidence/2026-09-20-bill-correction-routing.txt), [returns/payments regression 15 tests](evidence/2026-09-20-bill-correction-regression.txt) ผ่าน (จำนวนซ้ำข้ามรอบ ห้ามบวกเป็น test ไม่ซ้ำ). [Evidence validation 6 tests](evidence/2026-09-20-bill-evidence-validation.txt) ผ่านหลัง regenerate snapshot. Typecheck ผ่าน, lint 0 errors / 3 warnings เดิม; [isolated release](evidence/2026-09-20-bill-correction-release.txt) `.runtime/releases/2026-09-20T07-13-48-541Z` ผ่าน. [Source hashes/limitations](evidence/2026-09-20-bill-correction-final-context.json). Public supplier invoice queue เปิดผ่าน authenticated browser; correction flow ยังไม่มี browser acceptance.

Supplier correction handoff รอบล่าสุด: [matching + Calendar 20 tests](evidence/2026-09-20-bill-correction-calendar.txt) ผ่าน; Inbox แสดงสถานะและขั้นถัดไปตาม source. [Evidence validation 6 tests](evidence/2026-09-20-bill-calendar-evidence-validation.txt) ผ่านหลัง regenerate; typecheck ผ่าน, lint 0 errors / 3 warnings เดิม. [Isolated release](evidence/2026-09-20-bill-calendar-release.txt) `.runtime/releases/2026-09-20T07-16-33-662Z` ผ่าน. Work ใช้ named owner ปัจจุบัน; Calendar team แสดง correction แต่ mine ยังอิง creator/reviewer. ยังไม่ครบ A–J: matrix ปัจจุบันมี 80 Deferred และการตรวจราย capability/หลายบทบาทอีกมาก; ผลนี้ไม่ปลด simulation หรือ production gates ทั้งระบบ.

## รอบลด Deferred จาก 80 รายการ (2026-09-20)

ปัจจุบัน 77 Deferred: customer notes, quality cancellation และ CRM activities มี domain implementation เพิ่มแล้ว จึงย้ายสามรายการเป็น Implemented พร้อม gap ที่ยังเหลือ ไม่ใช่ acceptance ครบหรือ sign-off ทั้ง capability.

- Customer notes: source-order scope, create/correct/archive, immutable history, ป้องกันย้ายหลักฐานไปผิดลูกค้า. [คู่มือ](customer-contact-notes.md), [9 tests](evidence/2026-09-20-customer-notes-tests.txt).
- Quality cancellation: ขอ/ปฏิเสธ/ส่งใหม่/อนุมัติโดยคนละคน เฉพาะ hold ที่ยังไม่เคยตรวจ; ไม่สร้าง pass ปลอมและไม่ลบ stock/GL. [คู่มือ](quality-cancellation.md), [50 owner/adjacent tests](evidence/2026-09-20-notes-quality-regression.txt).
- CRM activity: นัดติดตามจาก order/quotation, current employee/role eligibility, ผู้รับปิดงานเอง, manager reassign/cancel มีเหตุผล, Work/Inbox/Calendar, history และ race guards. [คู่มือ](customer-followups.md), [28 tests](evidence/2026-09-20-customer-followup-tests.txt).
- ตัวเลข tests ซ้ำบาง suite ข้ามรอบ ห้ามรวมเป็นจำนวน unique. Schema ปัจจุบัน 345 tables. Typecheck ผ่าน; lint 0 errors / 3 warnings เดิม; isolated release `.runtime/releases/2026-09-20T08-52-01-332Z` ผ่าน. Browser บนฐานแยก run `6229eee3708d` กำลังตรวจ; public dev ตรวจหน้า customer notes และ Quality ได้โดยไม่สร้างธุรกรรมใน dev.

Browser run `6229eee3708d`: [observation](evidence/2026-09-20-customer-quality-browser.json) และ [source records](evidence/2026-09-20-customer-quality-browser-completed.json). Notes create/correct/archive ผ่าน; follow-up ผู้รับคนเดียวกับผู้สร้างเข้าจาก Work แล้ว complete; quality cancellation ผู้สร้างกับ Quality manager แยกบัญชีรับจาก Home และอนุมัติแล้ว stock จำนวน/มูลค่าเดิม ไม่มี inspection. Quality manager อ่าน notes ไม่ได้. ไม่ยกระดับทั้ง capability หรือ S01/S03/S11 เป็น accepted.

CRM เพิ่ม leads/opportunities จึงเหลือ **75 Deferred** (80→75 รวมรอบนี้); ยังไม่ใช่ acceptance ครบ A–J. [คู่มือ](crm-leads-opportunities.md). [10 tests](evidence/2026-09-20-crm-tests.txt) ผ่าน qualification/approved quotation→confirmed source sale→won, loss/reopen, scope/current eligibility, reassignment Work/Inbox/Calendar, immutable history และ forecast exact เกิน safe Number. Forecast แยก currency และปัด half-up ต่อ deal. Migration0215 dev ใช้แล้ว (348 tables), typecheck ผ่าน, lint 0errors/3warningsเดิม, isolated release `.runtime/releases/2026-09-20T09-13-50-000Z` ผ่าน. [Browser observation](evidence/2026-09-20-crm-browser.json) บน disposable run: create/qualify/lost/reopen/forecast ผ่านบัญชีเดียว; draft quote ไม่อยู่ในตัวเลือก approved. ไม่ถือว่าผ่าน browser หลายบทบาท. [Source records](evidence/2026-09-20-crm-browser-records.json), [context](evidence/2026-09-20-crm-context.json).

Customer 360 source timeline เพิ่มแล้ว เหลือ **74 Deferred** (80→74); [คู่มือ](customer-timeline.md). [8 tests](evidence/2026-09-20-customer-timeline-tests.txt) ผ่าน privacy/scope/pagination และ fulfillment/customer-return links จากธุรกรรมจริงที่กระทบ stock/GL. [Browser](evidence/2026-09-20-customer-timeline-browser.json) เห็น CRM/quotation timeline และกลับต้นทางได้; Quality manager ถูกปฏิเสธ CRM/timeline. Typecheck ผ่าน, lint0errors/3warningsเดิม, isolated release `.runtime/releases/2026-09-20T09-22-38-261Z` ผ่าน. ยังไม่รวม Support/Field Service/Legal/Finance/Portal และไม่ใช่ full acceptance. Disposable run `f7218583fd23` ปิดและลบฐานทดสอบแล้ว. Local/public health200 และ public CRM เปิดด้วย session ที่มีสิทธิ์ได้.

Support tickets เพิ่มแล้ว เหลือ **73 Deferred** (80→73). [คู่มือ](support-tickets.md), [63tests](evidence/2026-09-20-support-tests.txt), [context/release](evidence/2026-09-20-support-context.json). Ticket ผูก source order และ current eligible owner; accept/wait/resume/resolve แยกจาก independently reviewed staff-recorded customer confirmation; reject/reopen/cancel/reassign มี immutable evidence. Work/Inbox/Calendar และ Customer360 จำกัดตาม owner permissions. Regressionแรก87pass/5failเป็น navigation expectations เก่าที่ยังไม่มีสอง app ใหม่; แก้ expectation แล้ว63testsที่เกี่ยวข้องผ่าน. Migration0216 dev350tables, typecheck ผ่าน, lint0errors/3warningsเดิม, release `.runtime/releases/2026-09-20T09-36-47-226Z` ผ่าน.

[Browser3บัญชี](evidence/2026-09-20-support-browser.json) บน disposable run `fd3d5be619db`: sourceสร้างจากquotationเดิม→Supportagentเข้าจากMyactionsรับและresolve→sourceบันทึกphoneconfirmationจำลอง→SupportmanagerอีกคนรับจากMyactionsตรวจnote/versionแล้วclosev4. [DBrecords](evidence/2026-09-20-support-browser-records.json)ยืนยัน4events/คนละresolver-reviewer/contactหลังresolution และquotationยังdraftv1. ยังไม่ใช่Portal/ลูกค้าจริง/SLA/Supportcapabilityครบ. Full regressionกำลังรันแยก ไม่ยกระดับgateจากผลย่อย.

Full regression หลัง Support tickets และก่อน SLA: **95 files passed / 2 skipped; 803 tests passed / 3 skipped**. [ผล](evidence/2026-09-20-cross-department-full-803.txt) และ [scope/context](evidence/2026-09-20-cross-department-full-803-context.json). ผลนี้ไม่ครอบคลุม migration0217–0218 หรือโค้ด SLA ที่เพิ่มภายหลัง และ skipped ไม่ใช่ passed.

SLA watch เพิ่ม owner implementation แล้ว เหลือ **72 Deferred** (80→72); ไม่ใช่ 72 gaps ทั้งหมด เพราะ Implemented ยังมี acceptance/behavior ค้างตาม manifest. [คู่มือ](support-sla.md), [29tests/6files](evidence/2026-09-20-support-sla-tests.txt), [context](evidence/2026-09-20-support-sla-context.json). ปฏิทิน/วันหยุด/DST, independent policy approval, immutable snapshot, pause/resume/reopen, scoped Work/Inbox/Calendar และ durable breach escalation ผ่าน targeted regression. Worker พร้อมกันพบ collision ที่ primary key ในรอบแรก แก้ ON CONFLICT ให้ครอบคลุมทุก unique key แล้วทดสอบซ้ำผ่าน รวม downtime/ไม่มีผู้รับ/กลับมา eligible. Typecheckผ่าน, lint0errors/3warningsเดิม, release `.runtime/releases/2026-09-20T10-06-00-551Z` ผ่าน; migration0217–0218 applied dev (353tables).

[Browser](evidence/2026-09-20-support-sla-browser.json) และ [source records](evidence/2026-09-20-support-sla-browser-records.json): สามบัญชี ordinary login; creator draft→managerรับจากWorkอนุมัติ→ผูกเคส→agentรับจากWork→เกิดbreachตามเวลาจริง→waitทำclockpausedแต่breachอยู่→managerรับescalationจากWorkและบันทึกแผนติดตาม. เคสยังwaiting_customer v4, acknowledgementไม่ปิดเคส. Worker sweepเรียกตรงบนฐานจำลองเพื่อทดสอบ ไม่ใช่หลักฐาน uptimeproduction. Local/public health200 และหน้าpublic `/holdings/acme/support/sla` เปิดด้วยsessionจริงได้โดยไม่เขียนธุรกรรม. ยังเหลือ first response/default binding/multi-level escalation/acknowledged-owner reassignment/overnightcalendar/PortalEmail และ UAT จริง ไม่จัดเป็น external gate เพื่อซ่อน implementation gap.

Support incidents เพิ่มแล้ว เหลือ **71 Deferred** (80→71); [คู่มือ](support-incidents.md), [12tests/4files](evidence/2026-09-20-support-incidents-tests.txt), [context](evidence/2026-09-20-support-incidents-context.json). Source ticket→incident owner→investigation→root cause/mitigation→independent manager review/reject/reopen เชื่อม Work/Inbox/Calendar; link/unlink/relink มีเหตุผลและประวัติ. สองลูกค้าจาก quotation→ticket จริงใน integration ยืนยันว่าการปิด incident ไม่เปลี่ยนสถานะ/version ของเคสทั้งสอง. Typecheckผ่าน, lint0errors/3warningsเดิม, isolated release `.runtime/releases/2026-09-20T10-21-02-784Z`, migration0219 dev356tables.

[Browserสามบัญชี](evidence/2026-09-20-support-incidents-browser.json): ผู้สร้างสร้างticketและincident→agentเข้าจากMyactions2รับinvestigationและเสนอmitigation→managerเข้าจากMyactions1ตรวจแล้วclose. [DBrecords](evidence/2026-09-20-support-incidents-browser-records.json)ยืนยันincidentclosedv4, reviewerคนละคนกับmitigator, ticketยังopenv1; managerWork0. Browserใช้หนึ่งเคส; multi-customer/correction/reworkพิสูจน์ในintegration ไม่เหมารวมว่าbrowserผ่านทั้งหมด. Public devหน้า `/holdings/acme/support/incidents` เปิดได้. Cross-scope coordination, specialist dispatch, severity SLA, public status/email, attachments/postmortem และ UATยังค้างตามmanifest. Full regressionรอบหลังSLA+incidentกำลังรันแยก.

Full regression หลัง SLA+incidents จบแล้ว **99 files passed /2 skipped;817 tests passed /3 skipped** ([ผล](evidence/2026-09-20-cross-department-full-817.txt), [scope](evidence/2026-09-20-cross-department-full-817-context.json)). ชุดทดสอบถูกค้นตอนเริ่มก่อนเพิ่ม Support attachments; ระหว่างรันมีงาน attachments ต่อใน working tree จึงไม่ใช่ immutable final release acceptance และไม่นับว่า817ครอบคลุม attachments. งานไฟล์หลักฐานมี targeted evidence แยก. Skipped provider3รายการยังไม่ผ่านgate.

Finance generic controlled payment ปิดช่องว่างที่ draft payment เดิมไม่มี journal จึง allocate/reconcile ต่อไม่ได้: preparer→independent approver→third-person Treasury release→exact bank confirmation ทำ posted GL, multi-invoice allocation, invoice status, payment settlement และ bank match ใน transaction เดียว พร้อม version/idempotency/audit/decision evidence. [หลักฐาน 3 tests + related regression 10 tests + public UI](evidence/2026-09-21-finance-generic-controlled-payments.txt). หน้า posted invoice แสดงฟอร์ม controlled settlement และจำกัดตัวเลือกเฉพาะ bank control account. Multi-role resettable browser ส่งมอบในหลักฐานถัดไป; live provider และ business sign-off ยังไม่ผ่าน gate.

Finance generic controlled payment ผ่าน [resettable browser 3 บัญชี](evidence/2026-09-21-finance-controlled-payment-browser.txt) บน isolated run `ec2aae319bce`: preparer สร้างจาก posted payable→approver อนุมัติ→Treasury release และ confirm exact bank row→reload เห็น settled/outstanding 0/allocation/journal. [DB records](evidence/2026-09-21-finance-controlled-payment-browser-records.json) ยืนยันคนละ created/approved/released user, AP debit=bank credit 12500, invoice paid และ bank matched. จากนั้น two-close browser ยังผ่านบน run เดิม จึงไม่มีการปนยอดย้อนหลัง. Live provider, production และ business sign-off ยังไม่ผ่าน gate.

Finance multi-currency รองรับ invoice และ payment ที่ใช้ transaction currency ต่างจาก functional currency แล้ว. ผู้อนุมัติ invoice ระบุ recognition-date rate; Treasury ระบุ bank-date rate; allocation เก็บ transaction amount, carrying-base amount, settlement-base amount และ realized difference แยกกัน. การ confirm สร้าง payment journal กับ realized gain/loss journal, update invoice/payment/bank ใน transaction เดียว และ correction คืนทั้งสอง journal กับ functional outstanding. Statement ต่างสกุลเก็บ closing rate/base close สำหรับ reconciliation. [หลักฐาน 5 files / 12 tests](evidence/2026-09-21-finance-multi-currency.txt) รวม USD payable ที่รับรู้ 35 THB/USD และจ่าย 36 THB/USD จน AP เป็นศูนย์และลง realized loss THB 100. รายการต่างสกุลที่ยังเปิดจะบล็อก close อย่างชัดเจนจนกว่าจะทำ unrealized period-end revaluation; browser หลายบทบาท, provider และ business sign-off ยังเหลือ.

Backup/restore operational rehearsal เพิ่มจากเดิมที่พิสูจน์เฉพาะ Holding DB: [2 tests](evidence/2026-09-21-backup-restore-operations.txt), [full CLI result](evidence/2026-09-21-backup-restore-operations.json), [477-table business restore](evidence/2026-09-21-business-restore-records.json). Operator CLI จริงสำรองและคืน database/private files/application secret/release pointer, เก็บ current database credential manifest, สร้าง safety backup ก่อน restore และปฏิเสธ snapshot ที่ checksum เปลี่ยนโดยไม่ทิ้ง maintenance ค้าง. ยังไม่ใช่ production host, off-machine/disk-loss/WAL-PITR หรือ signed RPO/RTO acceptance.

Customer access/authority เพิ่มแล้ว เหลือ **68 Deferred**: [คู่มือ](customer-access.md), [23 tests](evidence/2026-09-20-customer-access-tests.txt), [portal actions 6 tests](evidence/2026-09-22-customer-portal-actions.txt), [context](evidence/2026-09-20-customer-access-context.json), [browser 3 บทบาท](evidence/2026-09-20-customer-access-browser.json) และ [source records](evidence/2026-09-20-customer-access-browser-records.json). Binding ผูก approved source+verified email+expiry, ต้อง independent review, first verified account ถูก bind ชัดเจน, ทุก portal view มี audit และ revoke แล้ว projection หายทันที. Portal ใช้ explicit field allowlist; acknowledge delivery/request support เขียน immutable evidence และแจ้ง staff ตาม order scope. Internal notes/cost/HR/Legal/private Support evidence ไม่ถูกส่ง. ยังเหลือ consent, delegated customer admin, reply/upload/email และ UAT จริง.

Customer duplicate review และ transactional merge เพิ่มแล้ว เหลือ **67 Deferred**: [คู่มือ](customer-duplicate-reviews.md), [focused merge tests](evidence/2026-09-22-customer-duplicate-merge.txt), [11-test baseline](evidence/2026-09-20-customer-duplicates-tests.txt) และ [context](evidence/2026-09-20-customer-duplicates-context.json). คู่ customer ถูก canonicalize กันยื่นกลับด้าน, independent current manager ตัดสิน distinct/duplicate/reopen และ merge safe orders/invoices/payments/refunds/carts/opportunities/expenses/POS sales ใน transaction เดียว พร้อม blocked lineage, version/idempotency/audit และ immutable-owner blockers; balance reconciliation, browser/business acceptance และ owner correction workflows ยังเหลือ.

Customer identity change เพิ่มแล้ว เหลือ **65 Deferred**: [คู่มือ](customer-identity-changes.md), [4 focused tests](evidence/2026-09-22-customer-identity-consent.txt) และ [context](evidence/2026-09-20-customer-identity-context.json). Legal name/tax registration/phone/structured address/notification email/consent ใช้ before/after snapshot, independent review, reject/resubmit และ customer-version guard; apply เปลี่ยน master กับ immutable evidence ใน transaction เดียว. External registry, browser/business acceptance ยังเหลือ.

Support sessions และ scoped capabilities เพิ่มแล้ว เหลือ **64 Deferred**: [คู่มือ](support-sessions-capabilities.md), [14-test regression](evidence/2026-09-20-support-sessions-tests.txt) และ [context](evidence/2026-09-20-support-sessions-context.json). Ticket owner ขอ session ให้ตนเองได้ไม่เกิน 8 ชั่วโมง, independent current manager อนุมัติ, แล้วออก allowlisted order-context/evidence-metadata grant แยกกัน; ทุก read ตรวจ expiry, ticket state, scope, IAM/assignment/HR eligibility และ active grant ใหม่. Revoke/reissue/session cascade มี optimistic version กับ immutable events. ยังไม่มี customer consent, remote-control/write/file-byte grant, Calendar, browser/business acceptance จึงไม่ยกระดับ S10.

Asset และ Maintenance lifecycle เพิ่มแล้ว เหลือ **54 Deferred**: [คู่มือ](asset-maintenance-lifecycle.md), [5-test regression](evidence/2026-09-20-asset-maintenance-tests.txt) และ [context](evidence/2026-09-20-asset-maintenance-context.json). Register→placement/custody→condition, asset-bound work order→schedule/start/downtime→block/resume→evidenced completion/cancel และ retirement แบบ independent review ใช้ version/scope/current eligibility/immutable history ร่วมกัน; owner ที่หมดสิทธิ์เข้า routing blocker และ reviewer reassign ได้. ยังขาด depreciation/Finance, parts/cost, recurring preventive schedule, Calendar, attachments, browser/business acceptance จึงเป็น Implemented 10 capabilities ไม่ใช่ scenario acceptance.

Field Service lifecycle เพิ่มแล้ว เหลือ **48 Deferred**: [คู่มือ](field-service-lifecycle.md), [46-test related regression](evidence/2026-09-20-field-service-tests.txt) และ [context](evidence/2026-09-20-field-service-context.json). Agreement independent activation→Support ticket handoff→schedule/reserve/dispatch→start/exception/resume→part issue→customer-confirmed completion→independent billing release ใช้ source/version/scope/current eligibility และ shared stock reservation ร่วมกัน; concurrent last-unit สำเร็จรายการเดียวและ owner ที่หมดสิทธิ์เข้า routing blocker. Billing release ยังเป็น source obligation ไม่ใช่ Finance invoice; warranty costing, Portal/Calendar/attachments, browser/business acceptance ยังเหลือ จึงเป็น Implemented 6 capabilities ไม่ใช่ S10 acceptance.

Collaboration files/relations/action cards เพิ่มแล้ว เหลือ **45 Deferred**: [คู่มือ](collaboration-work.md), [34-test related regression](evidence/2026-09-20-collaboration-work-tests.txt) และ [context](evidence/2026-09-20-collaboration-work-context.json). Conversation attachment ตรวจ membership ปัจจุบันและ bind ครั้งเดียว; typed relation เก็บ version snapshot/backlink และ action card เข้าคิว Work/Inbox พร้อม evidence completion กับ reviewed reassignment. ยังขาด source adapters ทุก domain, relation graph, retention/export/legal hold, Calendar และ browser/business acceptance จึงเป็น Implemented 3 capabilities ไม่ใช่ S12/S20 acceptance.

Learning lifecycle เพิ่มแล้ว เหลือ **40 Deferred**: [คู่มือ](learning-lifecycle.md), [56-test related regression](evidence/2026-09-20-learning-tests.txt) และ [context](evidence/2026-09-20-learning-context.json). Course independent publication→current-worker assignment→learner start→attendance/result evidence→completion/waiver→immutable eligibility handoff ใช้ version/scope/current eligibility และ Work/Inbox ร่วมกัน. Provider import/certificate expiry, classroom roster, Benefits consumer, Calendar และ browser/business acceptance ยังเหลือ จึงเป็น Implemented 5 capabilities ไม่ใช่ S18 acceptance.

Benefits lifecycle เพิ่มแล้ว เหลือ **35 Deferred**: [คู่มือ](benefits-lifecycle.md), [5-test owner/payroll/migration regression](evidence/2026-09-20-benefits-tests.txt) และ [context](evidence/2026-09-20-benefits-context.json). Plan และ enrollment window ต้อง independent review; dependent ต้อง verify ก่อน family election; election snapshot version/ยอดและผ่าน provider acknowledgement ก่อนคนละผู้รับรอง release immutable payroll input. Payroll preview รวม employee deduction/employer cost จาก effective released input ใน totals และ input snapshot จริง. Live provider, migration บริษัทจริง, Thai expert/shadow payroll, browserหลายบทบาทและ business acceptance ยังเป็น gate จึงไม่ใช่ S18 acceptance.

Talent lifecycle เพิ่มแล้ว เหลือ **29 Deferred**: [คู่มือ](talent-lifecycle.md), [38-test Talent/People/Work/migration regression](evidence/2026-09-20-talent-tests.txt) และ [context](evidence/2026-09-20-talent-context.json). Requisition independent approval→candidate consent/private retention→source-versioned application/owner recovery→interview proposal independent review→selected candidate→reviewed offer→acceptance evidence→immutable onboarding handoff เชื่อมเป็น source chain เดียว. Candidate portal/e-signature/background check, People consumer, retention erasure, browserหลายบทบาทและ business acceptance ยังเหลือ จึงไม่ใช่ S17 acceptance.

Agent governance เพิ่มแล้ว เหลือ **23 Deferred**: [คู่มือ](agent-governance.md), [9-test governance/assistant/migration regression](evidence/2026-09-21-agent-governance-tests.txt) และ [context](evidence/2026-09-21-agent-governance-context.json). Catalog/version independent approval→user/pool assignment→path routing→private authority-scoped session→immutable success/failure invocation evidence ครอบ assistant เดิม และ policy snapshot ห้าม AI approve/sign/pay. Source adapters/evaluation/retention, browser/provider/production และ business acceptance ยังเหลือ จึงไม่ใช่ S20 acceptance.

Marketing lifecycle เพิ่มแล้ว เหลือ **17 Deferred**: [คู่มือ](marketing-lifecycle.md), [14-test Marketing/CRM/Work/migration regression](evidence/2026-09-21-marketing-tests.txt) และ [context](evidence/2026-09-21-marketing-context.json). Form/content/domain/provider readiness ต้อง independent activation; lead snapshot consent/attribution/form version, engagement immutable และ CRM handoff เรียก owner service แบบ idempotent. Public hosting/provider delivery/webhooks/analytics, browser/provider/production และ business acceptance ยังเหลือ จึงไม่ใช่ S01/S16 acceptance.

Planning + Manufacturing lifecycle เพิ่มแล้ว เหลือ **6 Deferred**: [คู่มือ](planning-manufacturing-lifecycle.md), [15-test production/reservation/quality/migration regression](evidence/2026-09-21-production-tests.txt) และ [context](evidence/2026-09-21-production-context.json). Versioned BOM/formula→shortage/capacity/feasible date→independent recommendation→production order→shared reservation/actual issue→labor/output/scrap→independent QC→finished stock/cost/genealogy เชื่อม source เดียว. Incoming PO/order netting, detailed calendars, lot/serial/recall, WIP/GL, browserหลายบทบาทและ business acceptance ยังเหลือ จึงไม่ใช่ S03/S11 acceptance.

E-commerce lifecycle เพิ่ม owner implementation ครบหกรายการ ทำให้ทะเบียนมี **0 Deferred**: [คู่มือ](ecommerce-lifecycle.md), automated regression และ context อยู่ใน `evidence/2026-09-21-ecommerce-*`. Channel/listing independent review→versioned cart→idempotent checkout→Sales order→shared stock reservation→owner fulfillment link→invoice-gated return ใช้ source chain เดียว. ตัวเลข 0 หมายถึงทุก capability ในทะเบียนมี implementation evidence แล้ว; live storefront/payment/shipping provider, browserหลายบทบาท, UAT, migration และ production sign-off ยังเป็น gate แยก จึงยังไม่ใช่ S16 หรือ production acceptance.

รอบปิด Deferred ตรวจซ้ำทั้งระบบแล้ว: **114 test files passed / 2 skipped, 870 tests passed / 3 skipped**, typecheck ผ่าน, lint 0 errors / 3 warnings เดิม และ isolated release `.runtime/releases/2026-09-20T18-15-18-853Z` ผ่าน. [Full regression](evidence/2026-09-21-full-regression-zero-deferred.txt), [release build](evidence/2026-09-21-zero-deferred-release.txt), [context/checksums](evidence/2026-09-21-zero-deferred-context.json). Browser ผ่าน authenticated public dev สำหรับ E-commerce, Planning และ Manufacturing โดยไม่เขียนธุรกรรมจริง. Provider tests ที่ skipped, UAT, migration rehearsal และ production sign-off ยังคง pending ตาม gate และไม่ถูกนับรวมใน 0 Deferred.

Support private evidence เพิ่มแล้ว เหลือ **70 Deferred** (80→70): [คู่มือ](support-evidence.md), [27tests/4files](evidence/2026-09-20-support-evidence-tests.txt), [context](evidence/2026-09-20-support-evidence-context.json). จอง/upload/download/withdraw ใช้ private storage เดิม ตรวจ source version/current eligibility, signature/checksum, legal hold, retryเมื่อครบ10ไฟล์ และ frozenหลังresolution; expense uploadเดิมไม่ต้องส่งversionและยังผ่าน. [BrowserบัญชีSupportagent](evidence/2026-09-20-support-evidence-browser.json) เลือกCSV70bytesผ่านnativepicker แนบ→ถอนพร้อมเหตุผล→เสนอresolutionv5; [records](evidence/2026-09-20-support-evidence-browser-records.json) เก็บeventsและmetadataที่ถอนแล้ว. ยังไม่ใช่customerconfirmationหรือmulti-roleacceptance; Portal sharing/revocation, incidentfiles และ retentionยังค้าง. Release `.runtime/releases/2026-09-20T10-34-33-287Z` ผ่าน; public Supportqueueเปิดได้ ไม่มีธุรกรรมเขียนในpublicdev.

## Release 1 Inventory — lot, serial และ receipt quarantine (2026-09-21)

Product master เลือก `none`, `lot` หรือ `serial` ได้แล้วและเปลี่ยน mode หลังมี stock history ไม่ได้. Purchase receipt บังคับจัดสรรเต็มจำนวนไปยัง lot/serial, serial หนึ่งรายการเท่ากับหนึ่งหน่วย, ปฏิเสธเลขซ้ำ และสร้าง Quality hold จาก receipt disposition แบบ quarantine ได้ใน transaction เดียว. Sales delivery ต้องเลือก lot/serial ที่มีจริงและปล่อยจาก Quality แล้ว มูลค่าต้นทุนใช้ balance ของ lot ที่เลือก. Aggregate stock เป็น control total ของผลรวม lot quantity/value และ lot movements/fulfillment allocations แก้ย้อนหลังไม่ได้.

หน้าสต็อกแสดง On hand, Reserved, Quarantined, In transit และ Available แยกกัน; หน้า Quality เลือก hold ทั้งสินค้า หรือ lot/serial เฉพาะรายการได้. Demo Company เตรียมสินค้า lot/serial และ scenario ตามบทบาทเพิ่มแล้ว. [Automated 14 tests](evidence/2026-09-21-inventory-lot-serial-automated.txt), [automated context](evidence/2026-09-21-inventory-lot-serial-automated-context.json), [browser local/public](evidence/2026-09-21-inventory-lot-serial-browser.txt) และ [browser context](evidence/2026-09-21-inventory-lot-serial-browser-context.json). Typecheck ผ่าน, lint 0 errors/3 warnings เดิม, release `.runtime/releases/2026-09-21T08-00-39-762Z` ผ่าน.

ยังไม่ปิด Inventory ทั้งหมด: transfer, physical count, return, Production และ Field Service ยังต้องส่ง lot allocation เข้า owner workflow; landed cost และ structured scrap/return disposition ยังเหลือ. Browser รอบนี้ยืนยัน product/stock/Quality UI บน local desktop/mobile และ public desktop แต่ยังไม่ใช่ multi-role business acceptance ของ receipt→inspection→release→delivery ทั้งสาย.

## Release 1 Inventory — transfer, transit และ physical count ตาม lot/serial (2026-09-21)

Direct transfer และ staged transit รักษา lot/serial identity กับมูลค่าตั้งแต่ source allocation, dispatch, partial receipt จนถึง approved loss แล้ว. Physical count บังคับนับทุก lot/serial ที่มีอยู่ รองรับระบุศูนย์สำหรับรายการที่หาไม่พบและเพิ่ม lot/serial ที่พบใหม่พร้อม verified unit cost; posting ปรับ aggregate stock, lot balances, immutable movements และ GL variance ใน transaction เดียว. Version guards ปฏิเสธการอนุมัติเมื่อ stock หรือ lot เปลี่ยนหลังบันทึก.

[Automated 23 tests](evidence/2026-09-21-inventory-transfer-count-automated.txt), [public authenticated browser](evidence/2026-09-21-inventory-transfer-count-browser.txt) และ [context/checksums](evidence/2026-09-21-inventory-transfer-count-context.json). Typecheck ผ่าน, lint 0 errors/3 warnings เดิม, migrations 0240–0241 ใช้กับ dev แล้ว และ isolated release `.runtime/releases/2026-09-21T08-29-08-879Z` ผ่าน.

Inventory ยังไม่ครบทั้ง roadmap: returns, Production/Field Service adapters, landed cost, structured scrap/return disposition, browser หลายบทบาทและ business acceptance ยังเหลือ จึงไม่ยกระดับ gate เกินหลักฐานที่ตรวจจริง.

Receiver-selected transit allocation เพิ่มต่อแล้ว: receiving operator ต้องเลือก lot/serial จริงสำหรับ partial receipt และ loss report; server ตรวจว่า identifier อยู่ใน transport line, จำนวนรวมตรง line outcome, ไม่ซ้ำ, ไม่เกิน undisputed balance และ serial ต้องเป็นหนึ่งหน่วยเต็ม. Regression จงใจเลือก `ROAD-B` รับเข้าก่อน `ROAD-A` แล้วตรวจ destination balanceเพื่อยืนยันว่าไม่มี FIFO แฝง. Targeted transfer/transit regression 12 tests ผ่าน, typecheck และ lint ผ่าน, isolated release `.runtime/releases/2026-09-21T08-38-09-867Z` ผ่าน. Public authenticated stock-transit list เปิดได้ แต่ dev company ไม่มี staged record ที่กำลังเดินทาง จึงยังไม่มี browser transaction evidence ของฟอร์ม selection รอบนี้.

Customer และ supplier returns รักษา fulfilled lot/serial identity แล้ว. ผู้ขอเลือกได้เฉพาะ allocation จาก fulfillment เดิม, ระบบหัก prior non-rejected returns ราย lot, บังคับ serial เต็มหนึ่งหน่วย และ snapshot warehouse lot balance ไว้ตรวจ stale edit ตอนอนุมัติ. Supplier return ตัด selected lot; customer returnรับ lot เดิมกลับ พร้อม aggregate stock, lot balance, immutable lot movement และ stock reversal GL จากมูลค่าเดียวกัน. หน้า order แสดง returnable lot/serial และหลักฐาน identity/value ของ return. [Automated evidence](evidence/2026-09-21-lot-aware-returns.txt) 2 files/11 tests ผ่าน; migration 0242 applied dev, typecheck/lint/build ผ่าน และ release `.runtime/releases/2026-09-21T08-48-06-567Z` ผ่าน. Public authenticated order detail render ผ่านหลัง migration แต่ record ที่ตรวจยัง awaiting invoice จึงไม่ใช่ browser transaction acceptance ของ return.

## Release 1 Inventory / Quality — failed-stock scrap disposition (2026-09-21)

ผลตรวจ Quality แบบ fail มีทางจบที่ควบคุมได้แล้ว. ผู้สร้าง hold และผู้ตรวจห้ามขอทำลายเอง; Quality manager คนที่สามส่งจำนวนที่ถูกกักทั้งหมดเข้า stock count ที่สถานะ submitted พร้อม snapshot ของ stock และ lot/serial. Company Finance คนละคนต้อง approve แล้ว post ก่อนระบบเปลี่ยน hold เป็น disposed, ตัด quantity/value, ทำ lot เป็น scrapped เมื่อหมด, สร้าง immutable stock movement และลง Debit stock variance / Credit inventory ที่สมดุล. การ reject คืนงานให้ Quality ขอ disposition ใหม่ได้โดยไม่ลบประวัติเดิม; disposed hold และ disposition evidence แก้ย้อนหลังไม่ได้.

Automated regression ครอบ Quality, flow routing, physical count และ lot/serial 26 tests. ยังเหลือ structured measurement, attachment, return disposition, Production/Field Service adapters, multi-role browser และ business acceptance จึงคงสถานะ capability ตาม gate เดิม.

## Release 1 Quality — structured measurements and private evidence (2026-09-21)

Company Quality สามารถสร้าง product inspection specification ที่มี code, label, unit, minimum/maximum หลายค่าได้. ผู้สร้างห้าม activate เอง; reviewer คนละคนต้องบันทึกเหตุผลก่อนใช้จริง. Hold ใหม่จับ spec id/version ณ เวลาสร้าง แม้มาตรฐานรุ่นใหม่มาแทนภายหลัง ผู้ตรวจต้องกรอกค่าครบทุก metric และ server เป็นผู้คำนวณ pass/fail จาก limits. ค่าวัด, spec ที่ active/superseded และไฟล์ที่ bind กับ inspection แก้ย้อนหลังไม่ได้. ไฟล์ JPEG/PNG/WebP/PDF/CSV ใช้ private storage, signature/size/checksum, owner/version/current-authority guards เดิม และดาวน์โหลดต้องผ่าน Quality scope ของ hold.

Automated regression ครอบ Quality, private upload regression และ lot/serial 22 tests. Return disposition, escalation, browser หลายบทบาทและ business acceptance ยังเหลือ จึงคง capability gate เดิม.

## Release 1 Sales — versioned price lists and discount approval (2026-09-21)

Company Sales สร้าง price list แบบมีช่วงวันที่ ใช้ทั้งบริษัทหรือเจาะจงลูกค้า และกำหนดส่วนลดที่อนุมัติอัตโนมัติได้. ผู้สร้างห้าม activate เอง; sales manager คนละคนต้องตรวจราคา ภาษี และเหตุผลก่อนใช้จริง. ใบเสนอราคาและ sales order เลือกรายการที่ active ตามลูกค้า/วันที่ จับ price-list id/version, list price และ discount ต่อบรรทัดไว้เป็นหลักฐาน. ส่วนลดเกินเกณฑ์ต้องมีเหตุผลจากผู้เตรียมและเหตุผลอนุมัติจากผู้จัดการอิสระก่อนยืนยันเอกสาร.

Draft แก้ไขได้ก่อนอนุมัติด้วย optimistic version และ idempotency โดยระบบเก็บผู้เตรียมล่าสุดไว้บังคับ maker-checker. รายการราคาปัจจุบันกับรายการราคาอนาคตสามารถอยู่ในสถานะอนุมัติพร้อมกันได้; ระบบเลือกอัตโนมัติตามลูกค้า วันที่เอกสาร วันเริ่มใช้ และลำดับการอนุมัติ. Browser acceptance ใช้ Holding owner เตรียมและแก้ v1 → v2, Sofia Ramirez ใน session แยกอนุมัติเป็น v3 และใบเสนอราคาเดือนตุลาคมจับ snapshot v3 ได้จริง.

แก้ routing สำหรับเจ้าของบริษัทหรือผู้เชี่ยวชาญที่มี account assignment แต่ยังไม่มี HR engagement: บุคคลที่ถูกระบุเป็นเจ้าของเอกสารยังทำ draft ของตนเองได้ โดย candidate pools ตาม role ยังต้องอาศัย active staff เหมือนเดิม. Browser acceptance ส่ง QT-928B7E00B6FB544C50AED73B จากเจ้าของ, มอบหมาย Sofia พร้อมเหตุผล และอนุมัติจาก session ของ Sofia จนสถานะ `confirmed`.

[Automated and browser evidence](evidence/2026-09-21-sales-pricing-discount-controls.txt) ครอบ 4 files/22 tests และ public authenticated browser. Customer notification, independent-manager browser run และ business acceptance ยังเหลือ จึงไม่ยกระดับ Sales ทั้ง capability เกินหลักฐานที่ตรวจจริง.

## Release 1 Sales — proof of delivery (2026-09-21)

Sales delivery บังคับหลักฐาน JPEG/PNG/WebP/PDF แบบ private และข้อความยืนยันผู้รับหรือ carrier ก่อนตัด stock. ไฟล์ต้องเป็นของ sales order เดียวกัน, ไม่ถูก revoke, ใช้กับ delivery ได้ครั้งเดียว และความสัมพันธ์กับ fulfillment แก้หรือลบย้อนหลังไม่ได้. หน้า order แสดงหลักฐานของแต่ละ delivery พร้อมลิงก์ดาวน์โหลดที่ตรวจสิทธิ์ และแสดงไฟล์ที่อัปโหลดค้างไว้ให้เลือกต่อหลัง reload เพื่อรองรับการหลุดระหว่างกรอกฟอร์ม.

[Automated and browser evidence](evidence/2026-09-21-sales-delivery-evidence.txt) ครอบ regression ที่เกี่ยวข้อง 7 files/29 tests, focused lifecycle 1 test และ public browser สามบทบาท: Holding owner สร้าง/ส่ง/มอบหมาย, Sofia Ramirez อนุมัติ, Owen Brooks เปิดงาน fulfillment จาก Home แล้วเห็นฟอร์มที่บังคับไฟล์กับ confirmation note. เครื่องมือ browser รอบนี้ใส่ native file input ไม่ได้ จึงยังไม่นับ completed browser transaction; upload→link→ห้ามใช้ซ้ำพิสูจน์ใน integration. Customer notification และ business acceptance ยังเหลือ.

## Release 1 Sales — customer delivery notifications (2026-09-21)

Sales ส่งข้อความสถานะ delivery ได้จาก fulfillment จริงหลังมีอีเมลที่ตรวจแล้ว, consent เป็น granted และ manager เปิด adapter. ข้อความเก็บ snapshot ของ order, delivery, ลูกค้า, ผู้รับและ note แบบแก้ย้อนหลังไม่ได้; ก่อนแตะ provider ระบบตรวจอีเมลและ consent ปัจจุบันซ้ำ ถ้าเปลี่ยนจะ cancel. Worker รองรับ lease recovery, exponential retry และ provider idempotency key เดิม; หน้า order แสดง pending/sent/failed/cancelled, attempts, error และปุ่ม retry ที่ตรวจ version กับสิทธิ์. Sandbox adapter ไม่เรียก network ส่วน Resend ยังเป็น provider gate แยก.

[Automated, worker and public-browser evidence](evidence/2026-09-21-sales-customer-notifications.txt) ครอบ 5 files/16 tests. Holding owner เข้าคิวจาก public order SO-61D4770158966A46313FB419, อยู่ที่ route เดิม, เห็น pending จากนั้น detached jobs worker เปลี่ยนเป็น sent/attempt 1 และหน้า public แสดงผลหลัง reload. รอบนี้ยังไม่ใช่ real-email, Resend หรือ customer business acceptance; consent governance เต็มรูปแบบยังอยู่ Release 2 Customer Authority.

## Release 1 Sales / Inventory / Quality — customer-return disposition (2026-09-21)

สินค้าที่ลูกค้าคืนไม่กลับเป็น available stock ทันทีแล้ว. ผู้รับเรื่องต้องบันทึกสภาพ, หลักฐานสภาพ และวันครบกำหนดตรวจต่อบรรทัด; Commerce approver คนละคนคืน quantity/value และ lot/serial เดิมพร้อมสร้าง partial Quality hold ใน transaction เดียว. ของยังอยู่ใน stock control total แต่ถูกหักจาก available/reservable quantity. Quality inspector คนละคนตรวจ และ Quality manager คนที่สาม release เมื่อผ่าน; ถ้า fail ใช้ controlled scrap → independent Finance → stock count/GL เดิม. Condition, due date และ source link แก้ย้อนหลังไม่ได้ และหน้า order ลิงก์ไป disposition จริง.

[Automated and public-browser evidence](evidence/2026-09-21-customer-return-quality-disposition.txt) ครอบ Commerce returns, lot/serial, Quality, reservations และ E-commerce 5 files/27 tests. Public paid order SO-61F7BB88D96CAF6039D8F26A แสดงฟอร์ม condition/evidence/due date กับคำอธิบาย quarantine จริงโดยไม่สร้างรายการค้างใน demo. Independent multi-role public transaction และ business acceptance ยังเป็น gate แยก.

## Release 1 Inventory — shared reservation expiry escalation (2026-09-21)

Sales, POS, Field Service และ Manufacturing ใช้ `inventory_active_reservations` ชุดเดียวกันอยู่แล้ว. เพิ่ม scheduled escalation ให้ reservation ของ Sales, Field Service และ Manufacturing ที่ยังมียอดคงเหลือแจ้งผู้สร้าง/ผู้รับผิดชอบก่อนหมดอายุ 24 ชั่วโมง และแจ้งอีกครั้งเมื่อหมดอายุจนไม่กัน available stock แล้ว. Notification id ผูก source, reservation, expiry, phase และผู้รับ จึงเรียก worker ซ้ำได้โดยไม่สร้างข้อความซ้ำ. รายการที่ใช้หมด, release หมด หรือ source จบแล้วไม่ถูกเตือน และแต่ละข้อความเปิดกลับไปยัง source workflow ที่มีสิทธิ์จริงผ่าน Inbox.

[Automated and public UI evidence](evidence/2026-09-21-inventory-reservation-expiry-alerts.txt) ครอบ reservation/Field Service/Manufacturing/POS/E-commerce/lot-serial/recovery 7 files/46 tests รวม dedupe, warning, expiry, consumed/released exclusion และ actionable notification links. Typecheck และ scoped lint ผ่าน. Public authenticated Inbox render ผ่านโดยไม่เขียน notification จำลองลงบริษัท Acme. Capability `inventory/reservations` จึงยกระดับเป็น Flow accepted ตาม integration evidence; multi-role browser transaction และ business acceptance ยัง pending.

## Release 1 Finance — opening balance reconciliation (2026-09-21)

Opening import เชื่อม GL กับตารางลูกหนี้/เจ้าหนี้ใช้งานจริงแล้ว. ผังบัญชีแยก `receivable`/`payable` control account ออกจาก cash/inventory/tax control; preview ตรวจ active party, currency, reference, due date, account direction และยอด schedule ต่อบัญชีให้ตรง GL ทุกบาท. Import สร้าง draft opening journal กับ operational Finance invoices ใน transaction/idempotency boundary เดียว. ผู้อนุมัติ Finance คนละคน post ทั้งชุดพร้อมกันแล้วรายการเข้าสู่ Aging และรับ allocation ปกติ; reversal ทำให้รายการที่ยังไม่เคย settle เป็น void พร้อม GL และปฏิเสธเมื่อมี allocation history เพื่อกันยอดย่อยไม่ตรงบัญชีคุม.

[Automated and public-browser evidence](evidence/2026-09-21-finance-opening-reconciliation.txt) ครอบ 4 files/19 tests, typecheck, lint และหน้า public Finance Setup ที่ล็อกอินจริง. หน้า UI แสดง account 1200 เป็น receivable โดยอัตโนมัติและเปิดฟอร์ม customer/reference/due date/control/offset/amount ครบ โดยไม่ submit ข้อมูลลงบริษัท Demo. ยังเหลือ independent multi-role browser import→post→settle, สองรอบปิดบัญชีที่กระทบยอด, migration/provider/production และ business acceptance จึงคง capability gate ตามหลักฐานจริง.

## Release 1 Finance — controlled period-close reconciliation (2026-09-21)

Period close ตรวจยอดสะสม GL ถึงวันสิ้นงวดเทียบกับลูกหนี้และเจ้าหนี้แบบ as-of โดยหัก allocation ตามวันที่ชำระ และตรวจบัญชีธนาคารที่กำหนดเป็น `bank` กับ statement วันสิ้นงวด. ระบบปฏิเสธยอดต่าง, เอกสารต่างสกุลที่ยังไม่มีฐานแปลง, statement ขาดวันสิ้นงวด และข้อมูลที่เปลี่ยนหลังผู้เตรียมเริ่ม close. Snapshot เก็บ account, currency, GL, source, difference, source metadata, ผู้ตรวจ และ close version; หน้า Close แสดงตารางหลักฐานเดียวกับที่ service ใช้ตัดสิน.

[Automated and isolated-browser evidence](evidence/2026-09-21-finance-close-reconciliation.txt) พิสูจน์ AR/AP/bank difference เป็นศูนย์, maker-checker และปิด January/February ต่อเนื่องสองงวด รวมทั้ง stale-evidence rejection. Browser simulation ใช้ Finance preparer และ Finance approver คนละ session, ผ่าน reload และเห็นสองงวดเป็น closed จริง จึงยกระดับ Period close เป็น Flow accepted. Real-bank provider และ business acceptance ยังเป็น gate แยก.

## Release 1 Finance — period-end foreign-currency revaluation (2026-09-21)

หน้า Period close แสดง foreign AR/AP ที่ยังไม่ถูกประเมินค่าและไม่ยอมเริ่ม close จนกว่าจะมีรายการของงวดนั้น. Company Finance ระบุอัตราปิดงวดแยกตามสกุล, บัญชีกำไร/ขาดทุนในสกุลเงินใช้งาน และหลักฐานต้นทาง. ระบบจับยอดคงค้าง transaction/functional, เวอร์ชันเอกสาร และ source hash ไว้; ผู้จัดทำอนุมัติเองไม่ได้. ผู้อนุมัติ Finance คนอื่นเลือก approve เพื่อสร้าง balanced unrealized FX journal พร้อมปรับ carrying balance หรือ reject เพื่อจัดทำใหม่. ถ้ายอดคงค้างเปลี่ยนระหว่างรอ ระบบปฏิเสธเป็น stale และไม่ลงบัญชี.

[Automated evidence](evidence/2026-09-21-finance-fx-revaluation.txt) ครอบ payable loss, receivable gain, mixed exposure, self-review, stale source, rejection, idempotent approval, close reconciliation และ regression ของ controlled payment/payment correction/two-close รวม 4 files/9 tests. Local migration ใช้สำเร็จแล้ว. Public authenticated UI observation และ business acceptance ยังคงเป็น gate แยกจนกว่าจะตรวจรอบท้าย.

## Release 1 Organization — controlled branch and department lifecycle (2026-09-21)

Company Organization administrator สร้าง draft, submit, แก้หลัง reject และ cancel คำขอของตนเองได้แล้ว ผู้อนุมัติต้องเป็น administrator อีกคนและใช้ revision ปัจจุบันก่อน apply. Workflow รองรับสร้าง branch/department, rename, suspend และ reactivate; การ suspend ถูกปฏิเสธถ้ายังมี child unit, active staff, position, scoped assignment หรือ warehouse. การสร้างหน่วยงาน provision organization-managed channel ใน transaction เดียว และการเปลี่ยนชื่ออัปเดตชื่อห้อง ขณะที่สมาชิกยังคำนวณจาก placement กับ assignment ปัจจุบัน. ทุกขั้นมี command receipt, audit, outbox และ immutable lifecycle event.

[Automated and public UI evidence](evidence/2026-09-21-organization-node-lifecycle.txt) ครอบ Organization, organization channels, User Management และ IAM 4 files/41 tests. Typecheck, scoped lint และ local migration 0257 ผ่าน. Public authenticated Acme page แสดง branch/department จริง, status/revision, draft/submit controls และ review queue โดยไม่เขียนข้อมูลจำลอง. Organization ทั้ง 4 capabilities จึงเป็น Flow accepted ตาม integration และ browser-render evidence; ยังเหลือ independent two-session browser transaction และ company business acceptance.

## Release 1 Identity & Access — periodic access certification (2026-09-21)

Company IAM reviewer เปิด campaign ที่จับ snapshot ของ active assignment ทุกใบพร้อม user, role, scope, source, expiry และ version ได้แล้ว. ผู้เปิด campaign รับรองเองไม่ได้ และ reviewer รับรองสิทธิ์ของ Person ตนเองไม่ได้. แต่ละรายการเลือก keep หรือ revoke พร้อมเหตุผลและ fresh MFA; revoke เรียก protected revocation เดิม, ตัด active work context ทันที และยังรักษา last administrator/recovery safeguards. Source ที่เปลี่ยนหลังเริ่ม campaign จะ fail closed, ทุก decision เพิ่ม campaign version, event แก้ย้อนหลังไม่ได้ และ campaign ปิดอัตโนมัติเมื่อครบทุก assignment.

[Automated and public UI evidence](evidence/2026-09-21-iam-access-review-campaigns.txt) ครอบ IAM กับ User Management 2 files/31 tests. Typecheck, scoped lint และ migration 0258 ผ่าน. Public authenticated Reviews & audit แสดงฟอร์ม campaign และ audit จริงโดยไม่สร้างข้อมูลจำลอง. Capability `identity_access/access-reviews` จึงเป็น Flow accepted; due reminder, public multi-session transaction และ company business acceptance ยัง pending.

## Release 1 Identity & Access — reviewed invitation delivery (2026-09-21)

คำเชิญพนักงานสร้าง durable delivery job ใน control database พร้อมกับ invitation และรอ review `invitation.authorize` จากผู้ดูแลคนอื่นก่อนเข้าคิวส่งอีเมล. Worker สร้าง activation token ซ้ำได้จาก secret โดยไม่เก็บ token ดิบ, ใช้ provider idempotency key, lease recovery, exponential backoff, จำกัดหกครั้ง และ cancel เมื่อคำเชิญถูกปฏิเสธ หมดอายุ ถูก revoke หรือถูกคำเชิญใหม่แทนที่. ผู้ดูแลเห็น waiting/queued/sending/sent/failed/cancelled, จำนวนครั้ง, error และเวลาส่งใน Reviews & audit พร้อม retry failed delivery หรือ revoke คำเชิญด้วย fresh MFA และ IAM audit.

[Automated and public UI evidence](evidence/2026-09-21-account-invitation-delivery.txt) ครอบ worker และ IAM/User Management 3 files/34 tests. Typecheck, scoped lint, control migrations 0012–0013 และ authenticated public Reviews & audit ผ่าน. รอบนี้ไม่ส่งอีเมลจริงและไม่สร้างคำเชิญใน Acme; live Resend, independent multi-session invite→approve→activate, account recovery และ company business acceptance ยังเป็น gate แยก.

## Release 1 Identity & Access — durable account recovery (2026-09-21)

การขอเปลี่ยนรหัสผ่านไม่เปิดเผยว่ามีบัญชีหรือไม่ และสร้าง delivery job ที่เก็บ reset URL แบบ AES-256-GCM แทน plaintext. คำขอใหม่ยกเลิกลิงก์และงานส่งเดิมทันที; worker ใช้ lease recovery, provider idempotency, exponential retry สูงสุดหกครั้ง และเก็บ provider/error evidence โดยไม่เขียน token ลง event หรือ log. ลิงก์หมดอายุในหนึ่งชั่วโมง และเมื่อเปลี่ยนรหัสผ่านสำเร็จ ระบบยกเลิก session ทุกเครื่องพร้อม immutable completion event โดยมี fail-safe session deletion หาก audit เขียนไม่ได้.

[Automated and public-browser evidence](evidence/2026-09-21-account-recovery.txt) ครอบ recovery, Better Auth, invitation, IAM, User Management และ active work context 6 files/51 tests พร้อม Playwright ตรวจ invalid/superseded link UI. Typecheck, scoped lint และ control migration 0014 ผ่าน. Public page แสดงเหตุผลแบบไม่เปิดเผยข้อมูลบัญชีและลิงก์ขอใหม่โดยไม่มีช่องรหัสผ่าน. ยังไม่ส่งผ่าน live Resend, ไม่ทำ public multi-session reset จริง และยังไม่มี company business acceptance.

## Release 1 Identity & Access — access-review due escalation (2026-09-21)

Scheduled worker แจ้งผู้มีสิทธิ์ `iam.review` ระดับบริษัทก่อน campaign ครบกำหนดเจ็ดวัน และแจ้งอีกครั้งเมื่อ overdue. ผู้รับคำนวณจาก assignment chain, role revision และ principal ปัจจุบันทุกครั้ง; การรันซ้ำไม่สร้างข้อความซ้ำ และเมื่อถอนสิทธิ์แล้ว notification เดิมจะไม่ผ่านการตรวจ visibility ใน Inbox. ลิงก์พากลับ Reviews & audit พร้อม campaign anchor และหน้า campaign แสดงสถานะ overdue จากเวลาปัจจุบัน.

[Automated and public UI evidence](evidence/2026-09-21-iam-access-review-alerts.txt) ครอบ IAM, inventory notification regression, active work context และ User Management 4 files/47 tests พร้อม rerun IAM 22 tests หลังยืนยัน canonical public route. Typecheck และ scoped lint ผ่าน. Public authenticated Reviews & audit แสดง campaign form, due date, independent-review guidance และ empty state จริงโดยไม่สร้าง campaign ใน Acme. ยังเหลือ independent two-session public campaign transaction และ company business acceptance.

## Release 1 Home / Work — staff-task reminders and overdue escalation (2026-09-22)

งาน Staff task ที่ยังไม่เสร็จจะถูก sweep จาก scheduled worker ก่อนครบกำหนด 24 ชั่วโมง และเมื่อเลยกำหนดจะสร้าง escalation ถึงผู้รับผิดชอบ ผู้สร้าง และเจ้าของ project. ถ้ามี approval ที่กำลังรออยู่ ผู้อนุมัติคนนั้นได้รับ alert ด้วย. Notification ผูกกับ task version และ phase ทำให้ retry ไม่สร้างซ้ำ; ตอนเปิดจาก Inbox ระบบเรียก `getTask` เพื่อตรวจ scope และ authorization ปัจจุบันอีกครั้ง จึงไม่เปิดงานให้ผู้ที่ถูกถอนสิทธิ์.

[Automated evidence](evidence/2026-09-22-staff-task-alerts.txt) ผ่าน 1 file/2 tests ครอบ due-soon, overdue, owner escalation และ retry deduplication. Delegation ที่มี expiry มีหลักฐานแยกในหัวข้อถัดไป; policy-owned queue administration, resolution summaries, production scheduler และ multi-role browser/business acceptance ยังเหลือ.

## Release 1 Finance — recurring accrual schedules (2026-09-22)

Finance สร้าง draft schedule แบบรายเดือน/ไตรมาส/ปีได้ โดยตรวจ chart of accounts เดียวกัน, สกุลเงิน, วันเริ่ม–สิ้นสุด และยอดรวมก่อนบันทึก. ผู้อนุมัติที่ไม่ใช่ผู้สร้างต้อง approve ก่อนระบบจะเตรียมงวดถัดไป; การเตรียมสร้าง draft journal ที่สมดุลพร้อม source hash, period snapshot และ run event. การ post ตรวจ journal/period/schedule version และผู้อนุมัติปัจจุบันอีกครั้ง จากนั้นเลื่อน cursor งวดและปิด schedule เมื่อครบงวด. การยกเลิก, stale edit, duplicate prepare และ event แก้ย้อนหลังถูกกั้นด้วย version/idempotency/immutable history.

[Automated evidence](evidence/2026-09-22-finance-accrual-schedules.txt) ครอบ Finance accrual กับ Payroll regression 2 files/5 tests และ migration 0262 ที่ใช้จริงบนฐาน dev h001/h002. Local และ public health เป็น 200. หน้า public Finance Engines รอบนี้ redirect ไป sign-in เพราะ session เดิมหมดอายุ จึงยังไม่มี authenticated browser transaction และไม่ยกระดับเป็น business acceptance; live bank/provider, two-role schedule→post browser journey และ production Finance sign-off ยังคงเป็น gate แยก.

## Release 1 People & Payroll — controlled post-approval correction (2026-09-21)

Payroll preview ที่อนุมัติแล้วมี correction request แบบ maker-checker แล้ว. คำขอจับ run version, calculation revision, input digest, totals และเหตุผลเป็นหลักฐาน ผู้ขออนุมัติงานตัวเองไม่ได้ ผู้อนุมัติระดับ Company อีกคนต้องตัดสินพร้อมเหตุผล และระบบตรวจ source evidence ซ้ำก่อน commit. เมื่ออนุมัติ run กลับเป็น `calculating`, เพิ่ม version และต้องคำนวณ/อนุมัติใหม่ ขณะที่ review export รุ่นเดิมยังอ่านและตรวจ checksum ได้. Request/decision รองรับ idempotency, optimistic version, immutable events และ Inbox link ที่ตรวจ authority ปัจจุบันอีกครั้ง.

[Automated, build และ public UI evidence](evidence/2026-09-21-payroll-controlled-corrections.txt) ครอบ core calculation, independent decision, self-review denial, notification target, recalculation revision, retained export และ immutable history. เพิ่ม final-pay preparation จาก retirement clearance แล้ว: ตรวจ task owner/version, pay group/currency, last working day, สร้าง `final_pay` run ที่ scope เฉพาะคน และป้องกันการเตรียมซ้ำ. เพิ่ม Payroll Advance: ผู้ขอส่งคำขอ, ผู้อนุมัติคนอื่นตัดสิน, สร้างตารางหัก 1–24 งวด, รวมงวดที่ถึงกำหนดใน payroll input snapshot และป้องกัน event ลบ/แก้ไข. หน้ารายละเอียด approved run บน public dev แสดง Reviewed corrections, source-evidence form, next action และ calculation totals โดยไม่เขียน Acme data. Country packs/statutory adapters, multi-role browser transaction และ company business acceptance ยังเหลือ จึงไม่ถือว่า Payroll พร้อม production ทั้งโดเมน.

## Release 1 Home / Work — task delegation with expiry (2026-09-22)

ผู้สร้างหรือเจ้าของ project มอบหมาย task ที่ยังไม่จบให้ active staff ได้หนึ่งคน โดยต้องระบุเหตุผลและวันหมดอายุไม่เกิน 90 วัน. ระบบเพิ่ม task version และเขียน delegation, audit event, outbox และ notification ใน command เดียว; Flow เปลี่ยนผู้รับผิดชอบชั่วคราวให้ผู้รับมอบหมายทำ start/save-progress/complete และแนบหลักฐานได้. Revoke ใช้ version guard และ idempotency receipt เดิม ส่วน expiry จะตัด visibility/action access และคืน work ownership ให้ assignee เดิมโดยไม่ลบประวัติ.

[Automated evidence](evidence/2026-09-22-staff-task-delegation.txt) ผ่าน staff-work 25 tests และ regression staff-work + staff-task-alerts 27 tests. Migration 0263 ถูก apply ใน dev h001/h002, typecheck และ scoped lint ผ่าน. ยังไม่ปิด queue policy administration, resolution summaries, production scheduler หรือ multi-role browser/business acceptance; public session เดิมหมดอายุจึงไม่ได้อ้าง browser transaction.

## Release 1 Platform Administration — live operations visibility (2026-09-22)

Platform overview เปลี่ยนจาก sample preview เป็นข้อมูลจริงแบบ live จาก control plane และ Holding ที่ active. Platform role ที่ผ่าน `requirePlatformContext` เห็น heartbeat ของ control services, จำนวน outbox ที่รอ, กำลังถูก lease, dead-letter และ event เก่าที่สุดของแต่ละ Holding; ถ้าฐานใดอ่านไม่ได้จะแสดง `unavailable` แยกโดยไม่ซ่อน Holding อื่น. API `/api/v1/platform/operations` ใช้ private no-store response และหน้าเดียวกันใช้ summary owner เดียวกัน จึงไม่เปิดสิทธิ์ customer data เพิ่ม. Platform administrator กู้ dead-letter ได้ทีละงานจากหน้าเดียว โดยต้องระบุเหตุผล, expected attempts/error และ idempotency key; ระบบล้าง lease/error, คืนงานเข้าคิวด้วย transaction เดียวใน Holding พร้อม command receipt และ audit event. Auditor ยังเป็น read-only และ stale snapshot ถูกปฏิเสธ.

[Automated evidence](evidence/2026-09-22-platform-operations-dashboard.txt) ผ่าน 4 tests, typecheck และ scoped lint. Provisioning, migration, temporary-access commands, public multi-role browser และ production RPO/RTO acceptance ยังเหลือ; browser session เดิมหมดอายุ.

## Release 2 Customer 360 — Field Service and Finance source state (2026-09-22)

Customer timeline เชื่อม owner records เพิ่มสองแหล่ง: Field Service agreement events และสถานะล่าสุดของ Finance invoice. Directory และ detail จะรวมลูกค้าที่มีแหล่งดังกล่าวตาม Company/Branch/Department scope; Field Service ต้องมีโมดูล `field_service` และ Finance ต้องมี `finance.read`. Projection ส่งเฉพาะ id, title, status, version, time และ source link จึงไม่คัดลอกเหตุผล/ต้นทุน/ข้อมูลลับ และต้นทางยังตรวจสิทธิ์ซ้ำเมื่อเปิดรายละเอียด.

[Automated evidence](evidence/2026-09-22-customer-timeline-field-service-finance.txt) ผ่าน customer timeline 5 tests รวม scope, source links, privacy และ permission gates. Timeline เชื่อม Legal order-linked records เพิ่มแล้ว โดยเคารพ legal.read กับ internal/membership classification. Portal/customer authority, invoice transition history, external integrations และ multi-role/browser/business acceptance ยังเหลือ.

## Release 2 — Field Service → Finance invoice handoff (2026-09-22)

Independent billing release จาก service order ที่ completed แล้วสร้าง immutable billing obligation และ draft receivable invoice ใน Finance ใน transaction เดียว. Invoice ผูก `source_type=field_service` กับ service order, customer, currency, amount, receivable control account และ revenue account; command ใช้ idempotency/version/audit เดิมของ Field Service และปฏิเสธการ release หาก Finance account setup ไม่ครบ. Finance reviewer ยังต้อง post และดำเนิน collection ตาม workflow แยก.

[Automated evidence](evidence/2026-09-22-field-service-finance-handoff.txt) ผ่าน `tests/field-service.test.ts` 4 tests รวม invoice linkage และ account mapping. Warranty/chargeable policy, posting/collection, browser และ business acceptance ยังเหลือ.

## Release 1 Procurement / Finance — one supplier invoice across multiple POs (2026-09-22)

Supplier invoice matching now supports one reviewed invoice covering posted goods receipts from multiple purchase orders. Capture loads each source order through the existing authorization path, requires the same supplier, currency, branch and department, checks every receipt version and posted goods GL, and matches every selected receipt line exactly once. The AP handoff prepares one payable invoice from the consolidated totals and links every selected receipt to that invoice; correction and posting guards continue to release or settle all linked receipts atomically.

Finance can start this workflow from the real Supplier invoice reviews page. The capture form lists eligible posted receipts across purchase orders, keeps incompatible supplier/scope rows disabled, and shows PO and receipt references in the review detail. The existing order page remains available for one-order matching.

[Automated evidence](evidence/2026-09-22-consolidated-supplier-invoice-cross-po.txt) passed `tests/supplier-bill-matching.test.ts` and `tests/purchase-to-payment.test.ts` (10 tests). Independent browser business acceptance, live provider, statutory and production backup/restore gates remain separate.

## Release 1 Procurement / Finance — consolidated bill browser acceptance (2026-09-22)

The resettable browser demo now exercises the same cross-PO path through the actual Finance workspace. Eight role contexts complete the ten-unit request, split 6 + 4 RFQs/POs, receipts and goods GL posting. Finance selects both receipts on Supplier invoice reviews, captures one consolidated supplier invoice, obtains independent review, prepares/posts one THB 250,000 AP invoice, exports one supplier instruction, imports one exact sandbox bank debit and confirms the invoice as paid with zero outstanding. The demo run is destroyed after the test.

[Browser evidence](evidence/2026-09-22-purchase-request-consolidated-purchase-to-payment-browser-acceptance.txt) passed 1/1 in run `7dec41784873`. Live providers, statutory controls, production operation, backup/restore certification and company business sign-off remain separate gates.

## Release 1 Sales / Finance — Sales to Collection browser acceptance (2026-09-22)

The resettable browser demo now exercises the connected Sales-to-Collection path through the actual workspace. Independent Sales operator, Sales manager, warehouse, Finance preparer/approver, and Treasury contexts submit and approve a quotation, convert and approve the order, retain valid PDF proof of delivery, post the goods-movement GL, prepare and post the customer invoice, and record the receipt. After reload the invoice is paid with THB 0.00 outstanding, and the delivery evidence remains on the order timeline.

[Browser evidence](evidence/2026-09-22-sales-to-collection-browser-acceptance.txt) passed 1/1 in isolated demo run `7216c4a30af1`. Live payment/email providers, statutory controls, production operation, and company business sign-off remain separate gates; the return/refund continuation is recorded in the next section.

## Release 1 Sales / Finance — customer return and controlled refund browser acceptance (2026-09-22)

The resettable browser demo now continues the accepted sale through return and refund. Independent Sales operator, Sales manager, Warehouse manager, Finance preparer/approver, Treasury, and Holding owner contexts submit and approve a customer return, post the stock-reversal and credit-note GL journals, approve the refund obligation, export the refund instruction, match the exact sandbox bank transaction, and settle the refund with zero outstanding. Return evidence and the refund bank match remain linked to the same order history.

[Browser evidence](evidence/2026-09-22-sales-return-refund-browser-acceptance.txt) passed 1/1 in isolated demo run `fd43f46711e7`. This is simulation evidence only; live payment/bank providers, statutory controls, quality release or scrap disposition, production operation, exception/retry acceptance, and company business sign-off remain separate gates.

## Release 1 Organization — guided readiness checklist (2026-09-22)

หน้า Organization แสดง readiness checklist จากข้อมูลจริงของบริษัท โดยแยกสถานะพร้อมใช้งานกับรายการที่ต้องเตรียม พร้อมระบุ owner, เหตุผล และลิงก์ไปยังหน้าตั้งค่าที่แก้ได้ ครอบคลุม staff levels, branch/department structure, CEO/reporting anchor, staff profiles และ approval coverage ทำให้ผู้ดูแลเห็นจุดที่บล็อก workflow ก่อนเปิดใช้งานและรู้ว่าต้องส่งต่อให้ใคร

[Automated and authenticated public-browser evidence](evidence/2026-09-22-organization-readiness-checklist.txt) ผ่าน organization และ organization-chart 17 tests, typecheck, scoped lint และ diff check. Public Acme workspace แสดง 3 รายการพร้อม และ 2 รายการต้องเตรียม (7 staff profiles และ 4 reporting teams) โดยไม่ได้แก้ข้อมูลบริษัท. Independent multi-role transaction, external-provider validation และ company business acceptance ยังเป็น gate แยก.

## Release 1 Finance — opening-balance browser handoff (2026-09-22)

Finance opening balances เลื่อนจาก `Implemented` เป็น `Flow accepted` หลังทดสอบผ่าน demo browser แบบ isolated จริง: Finance preparer เลือก chart, กรอกยอด THB ที่สมดุล, preview reconciliation และสร้าง draft opening journal; Finance approver คนละบัญชีเปิด journal ที่บันทึกแล้ว อนุมัติและ post จากหน้า Finance จริง จากนั้น API ยืนยัน `source_type=opening_import` และการรันถูกทำลายหลังจบเพื่อกันข้อมูลรั่วไปยังบริษัทจริง

[Browser evidence](evidence/2026-09-22-finance-opening-balance-browser-acceptance.txt) ผ่าน 1/1 test ใน run `4a8543f96719`. Opening-balance settlement, live/provider validation, production operation, backup/restore และ company business sign-off ยังคงเป็น gate แยก.

## Release 1 Finance — opening-balance settlement integrity (2026-09-22)

ระหว่างทดสอบ settlement พบและแก้ bug ใน `importOpeningTrialBalance`: invoice ของ opening subledger ไม่ได้บันทึก `base_total_minor` และ `base_outstanding_minor` ทำให้การรับชำระ THB เดียวกันอาจถูกคำนวณเป็น realized FX gain. ระบบบันทึก functional balances ตั้งแต่ import แล้ว และ regression ยืนยันว่า Treasury รับชำระจน invoice เป็น paid, ยอดคงเหลือเป็นศูนย์ และการ reverse opening journal หลัง settlement ถูกปฏิเสธด้วย `OPENING_SUBLEDGER_SETTLED` เพื่อรักษาประวัติและยอดบัญชีต้นทาง

[Automated evidence](evidence/2026-09-22-finance-opening-settlement.txt) ผ่าน `tests/finance-opening-balances.test.ts` 6 tests. Live bank/provider, production operation, backup/restore และ company business sign-off ยังคงเป็น gate แยก.

## Release 1 Inventory — browser accepted staged transfer (2026-09-22)

Inventory Issues & movements now has a real Demo Company browser acceptance run in addition to the owner-service regression. Separate source and receiving warehouse-manager accounts created a 3-unit staged transfer, submitted it, approved it independently, recorded dispatch evidence, received one unit, reloaded the record, and received the remaining two units. The completed transport detail reconciled to 3 received units and no loss; the downstream stock projection showed 3 units and THB 10.00 at the receiving warehouse. The isolated run and its databases were destroyed after verification. [Browser evidence](evidence/2026-09-22-inventory-transit-browser.txt) and [ledger record](../status/business-acceptance-evidence.json) are retained. Intercompany documents, customer proof of delivery, provider/production operation and company business acceptance remain separate gates.

The same staged transfer was rerun on 2026-09-23 with a fresh Demo reset (`da74fb84b66a`) after the shared Finance queue work. The browser path again passed source creation, independent approval, dispatch, one-unit receipt, two-unit completion, reload persistence and target stock reconciliation. [Fresh browser evidence](evidence/2026-09-23-stock-transit-browser.txt) is recorded separately; this confirms repeatability and does not promote the flow to production or Company acceptance.

## Release 1 Finance — receivables and payables aging browser acceptance (2026-09-22)

The Finance invoice workspace now presents live aging summaries for both receivables and payables. Each summary groups open posted documents by overdue bucket, keeps amounts separate by currency, shows document count and oldest due date, and links directly to the filtered queue. An authenticated Demo Company browser check matched the receivable summary to the protected `aging` API response (current THB 300.00) and rendered the payable summary (two current THB 325.00 documents). [Browser evidence](evidence/2026-09-22-finance-aging-browser.txt) and [ledger record](../status/business-acceptance-evidence.json) are retained. Exact AR/AP-to-control reconciliation, schedules, provider/production operation, and company business acceptance remain separate gates.

## Capability register reconciliation — three Release 1 flows promoted to Flow accepted (2026-09-22)

The capability register now reflects existing acceptance evidence instead of leaving three completed internal journeys at `Implemented`. `finance/feature-receivables-aging` is backed by opening-balance, FX settlement/revaluation, Sales-to-Collection, and the authenticated aging workspace/API checks. `procurement/requisitions` is backed by the independent source-linked 6 + 4 purchase browser journey, consolidated cross-PO invoice/payment browser evidence, and owner concurrency tests. `procurement/feature-supplier-history` is backed by immutable commitment, replacement/withdrawal, partial-receipt owner tests and the four-account browser journey. Their remaining live-provider, statutory, production-operation and company business gates stay explicit; no capability was promoted based on mockup-only evidence.

## Release 1 Finance — reports linked to supporting ledger entries (2026-09-22)

Finance Reports now makes the report-to-source handoff explicit. Statement cards and the trial balance link to the selected chart’s Accounting journal workspace while preserving the chart identifier, so a reviewer can move from a total to the supporting posted entries without losing the reporting boundary. An authenticated Acme browser observation rendered live USD totals and verified the links on the public development hostname. [Browser evidence](evidence/2026-09-22-finance-reports-linked-ledger-browser.txt) and [ledger record](../status/business-acceptance-evidence.json) are retained. Full report reconciliation, close approval, provider/production operation and company business acceptance remain separate gates.

## Release 1 Platform Administration — backup and recovery flow accepted (2026-09-22)

The capability register now reflects the existing operational recovery evidence. The macOS operator backup/restore flow passed isolated PostgreSQL 18 rehearsals for database dumps, private files, application secret, retained credentials, release selection, automatic pre-restore safety backup and checksum fail-closed behavior. A source-linked Holding restore also verified table digests, stock reservations and application invariants without reposting transactions. `platform_administration/feature-backup-recovery` and `feature-infrastructure` are now `Flow accepted`; off-machine durability, WAL/PITR, live restart, every production database and signed RPO/RTO remain explicit gates. The focused regression passed 3 files / 3 tests in this run. [This rerun evidence](evidence/2026-09-22-backup-restore-regression.txt) and the existing [backup evidence](evidence/2026-09-21-backup-restore-operations.txt) remain retained.

## Release 1 Finance — usable chart-of-accounts setup controls (2026-09-22)

Finance setup now exposes the existing chart and account owner APIs through the real workspace for authorized Finance users. A user can create a reporting chart, add a posting or control account, inspect the selected chart’s account table, preview a CSV import with row-level validation before submitting an idempotent upsert, and activate or deactivate an account with a reason and expected-version guard. The selected chart remains in the setup URL after creation, while account creation, status changes and import refresh the connected opening-balance workspace. The isolated Demo browser journey exercised all three mutations against the protected APIs without changing Acme data, so `finance/feature-chart-of-accounts` is now `Flow accepted`. [Browser evidence](evidence/2026-09-22-finance-chart-setup-browser.txt) and [ledger record](../status/business-acceptance-evidence.json) are retained. Full chart migration, opening/subledger reconciliation, period close, provider/statutory setup, production operation and Company business acceptance remain separate gates.

## Release 1 Finance — Reports source-trace browser acceptance (2026-09-22)

The Reports workspace now has an isolated Demo browser acceptance for the complete read path: protected statements and trial-balance responses are checked for the selected chart, the rendered trial-balance row count matches the API, and every statement total links to Accounting with the same chart and journal-entry anchor. The run used the real Finance approver interface and was destroyed after verification. [Browser evidence](evidence/2026-09-22-finance-reports-browser.txt) and [ledger record](../status/business-acceptance-evidence.json) are retained. Period-close evidence, notes, consolidation, provider/statutory operation, production controls and Company business acceptance remain separate gates.

## Release 1 Collaboration — record-linked action-card browser acceptance (2026-09-22)

The real Cross-record collaboration workspace now has an isolated two-role Demo journey for a source-linked next action. A Holding owner creates an action card against the Demo Sales quotation and assigns it to a separate employee account; after reload, that employee sees the same card, records completion evidence, and completes it through the owner-only action path. The command keeps the existing source version, authorization and idempotency guards. [Browser evidence](evidence/2026-09-22-collaboration-action-card-browser.txt) and [ledger record](../status/business-acceptance-evidence.json) are retained. More source adapters, Calendar linkage, retention/export, provider/production operation and Company business acceptance remain separate gates.

## Release 1 Collaboration — record relation browser acceptance (2026-09-22)

The Cross-record collaboration workspace now has an isolated Demo journey for a real cross-department backlink. A Holding owner creates a Support case from the Demo Sales quotation, assigns it to the Support agent account, links the quotation to the case as a typed `related` relation, reloads the relation so both source and target remain visible, and removes it with correction evidence. The journey uses the existing source-version, authorization, idempotency and immutable-history controls. [Browser evidence](evidence/2026-09-22-collaboration-record-relations-browser.txt) and [ledger record](../status/business-acceptance-evidence.json) are retained. Additional source adapters, relation graph views, retention/export, provider/production operation and Company business acceptance remain separate gates.

## Release 1 Collaboration — threads and mentions browser acceptance (2026-09-22)

The real Home workspace now has an isolated two-role Demo journey for persisted team conversation. A Holding owner publishes an update mentioning the Support agent; an employee account sees it after reload, replies, toggles Thanks and follows the conversation, then reloads and still sees the reply and both states. The run exercises the existing feed APIs and person-mention resolver through the actual interface. [Browser evidence](evidence/2026-09-22-collaboration-social-browser.txt) and [ledger record](../status/business-acceptance-evidence.json) are retained. Record/agent mention adapters, retention/export, provider/production operation and Company business acceptance remain separate gates.

## Release 1 People & Payroll — payroll preview and independent approval (2026-09-22)

The Demo Company now starts with a monthly THB pay group, an active pay-group engagement and separate Payroll preparer/approver assignments. The real People → Payroll interface creates a run, calculates a gross-pay preview with live totals, routes the pending review through the workflow-responsibility page, and lets a different Payroll approver approve it; the preparer sees the approved run after reload. Global payroll-core evidence now includes controlled Finance GL posting; statutory country packs, bank/provider execution, production operation and Company business acceptance remain separate gates. [Browser evidence](evidence/2026-09-22-payroll-runs-browser.txt) and [ledger record](../status/business-acceptance-evidence.json) are retained.

## Release 1 People & Payroll — Finance GL posting (2026-09-23)

Approved global payroll runs now have a real Finance handoff. A Finance approver selects an active chart, open period, posting date and explicit expense/payable accounts from the payroll run page; the service creates one balanced `payroll` journal, marks the run and its items posted atomically, and links the journal through `hr_payroll_postings`. The same run can then create a payroll payment batch, require a different Treasury reviewer, export a sandbox instruction file, and confirm each bank result through `settled`; the flow remains source-linked and reloadable at every state. The commands enforce the Finance workflow owner, open-period and active-account checks, expected-version concurrency, immutable audit/outbox history and idempotent retry. Automated evidence covers deductions, employer contributions, exact debit/credit reconciliation, duplicate prevention and payment settlement. [Evidence](evidence/2026-09-23-payroll-gl-posting.txt) and [ledger record](../status/business-acceptance-evidence.json) are retained. Country statutory adapters, live bank/provider execution and Company business acceptance remain separate gates.

The same handoff now passes as a fresh five-role browser scenario in the isolated Demo Company: Payroll preparer → Holding owner routing → Payroll approver → Finance approver → Treasury. The test uses the real pages and authenticated API responses, checks the Treasury payment-only view does not expose payroll items, and verifies the settled batch survives reload. Demo reset seeds explicit payroll flow owners so the scenario remains deterministic while real-company routing stays configurable. [Browser evidence](evidence/2026-09-23-payroll-payment-browser.txt) is retained. Country statutory adapters, live bank/provider execution and Company business acceptance remain separate gates.

Finance now has one live Payment obligations queue for generic payments, supplier batches, refunds, payroll batches and expense/contractor batches. Each row shows source, due date, remaining amount, status and next action, then links back to the owning workflow; access is filtered so a Finance preparer does not receive payroll batch rows without payment-read authority. The Demo payroll browser path opens this queue before Treasury settlement and verifies the source row is actionable.

## Release 1 Collaboration — conversation attachment browser acceptance (2026-09-22)

The real Inbox now has an isolated two-role Demo journey for protected conversation files. A Holding owner creates a direct chat with the Support agent, sends a CSV attachment, and the employee opens the same conversation, downloads the protected bytes and still sees the file after reload. The owner-service regression remains the source for draft isolation, one-time binding and membership revocation; this browser run verifies the actual upload/send/read path. [Browser evidence](evidence/2026-09-22-collaboration-attachments-browser.txt) and [ledger record](../status/business-acceptance-evidence.json) are retained. Retention/export, legal hold, provider/production operation and Company business acceptance remain separate gates.

## Release 1 Operations — delegation handback and retained history (2026-09-22)

Temporary task delegation now retains the operator's reason for revocation and exposes authorized history with participants, dates and active/expired/revoked status. Earlier revocations without stored reasons are labelled explicitly; revoked evidence is immutable. An isolated four-account browser journey verifies delegation, partial progress, revocation, immediate loss of the delegate's read access, accountable handback and independent completion approval, including reload persistence. The service regression passes 27 tests with expiry, stale-version rollback, retry deduplication and unauthorized history access. The resettable Demo guide includes role-specific steps. [Evidence](evidence/2026-09-22-task-delegation-history.txt) and [scoped records](evidence/2026-09-22-support-and-delegation-history-browser-records.json) are retained. General queue policies, exception/resolution summaries, production scheduling and Company business acceptance remain.

## Existing Support flow — correction and confirmation browser coverage (2026-09-22)

The isolated Demo now exercises source-linked case creation, acceptance, waiting/resume, proposed resolution, independent return for correction, a second resolution, staff-recorded customer contact, independent confirmation, reopen and cancellation. Contact time accepts seconds so a note made after resolution in the same minute does not get truncated before the resolution. Browser reloads preserve contact evidence and the final version 10 history. [Evidence](evidence/2026-09-22-support-lifecycle-browser.txt) is partial browser acceptance of the existing flow; Portal replies, email, first-response SLA, incident/postmortem evidence, Field Service fulfillment and business acceptance remain separate. The combined Support/delegation browser run passed both scenarios and its isolated run was destroyed. Public read-only checks confirmed Support/contact screens and the authorized Work empty state; this session has no visible task detail records.

## Release 1 Organization — reviewed setup through the real interface (2026-09-22)

Branch/department proposal forms now retain which submit button was used; the previous FormData construction sent Save draft and Submit for review to an unsupported route. Company administrators can now search and paginate open, applied and cancelled requests, inspect named immutable decision history, and retry failed commands without losing inputs. The read surface and command guard require Company authority in the Company workspace; linked accounts cannot self-review.

Independent Demo owner/HR accounts passed draft, submit, rejection, correction, approval, rename, department creation, blocked branch suspension, cancellation, dependency-ordered suspension and reactivation on desktop and mobile. The protected ordinary employee view excludes Company review evidence. The owner-service regression passes 14 tests, including a 21-request pagination case, literal search, stale-version rollback and scoped/linked-account authorization. The Demo guide now includes an eight-step reviewed-organization-setup exercise. [Browser/service evidence](evidence/2026-09-22-organization-lifecycle-browser.txt) and [persisted records](evidence/2026-09-22-organization-lifecycle-browser-records.json) are retained. Public verification was read-only; initial Company provisioning, archive/relocation, Work/Inbox review routing and Company business sign-off remain. The shared mobile header's overlapping context text remains a UI gap even though the organization controls are operable.


### 2026-09-22 — Organization Work/Inbox handoff

เชื่อมคำขอเปลี่ยนองค์กรเข้าคิว Work ตามผู้ขอและผู้อนุมัติ พร้อมแจ้งเตือน submit, return, apply และ cancel ใน transaction เดียวกับ owner command. ลิงก์ changeId เปิดตรงคำขอและยังใช้ได้หลังเปลี่ยนสถานะหรือข้ามหน้ารายการ. Work, Inbox และหน้าหลักฐานตรวจ Company grant ปัจจุบัน รวม expiry/revocation และ linked-person self-review ที่ฐานข้อมูลอีกครั้ง.

Service/guide regression ผ่าน 17 tests (47.84s); browser desktop/mobile ผ่าน 2 scenarios (1.0m) โดยเปิด review จาก Work และ correction จาก Inbox ก่อนทำ lifecycle เดิมจนครบ. เก็บ 16 requests, 38 immutable events และ 36 notices ใน `docs/testing/evidence/2026-09-22-organization-work-inbox-records.json`; รายละเอียดและข้อจำกัดอยู่ใน `2026-09-22-organization-work-inbox.txt`. Typecheck และ scoped ESLint ผ่าน. Public Work แสดงตัวกรองใหม่ผ่าน session จริง; local/public health HTTP 200. Goal Release 1–4 ยัง active และไม่ได้ยกระดับ Company/provider acceptance.

งานถัดไป: แก้ shared mobile header ที่ข้อความทับกัน จากนั้นปิดช่องว่าง setup/provisioning และการเปลี่ยนองค์กรที่ยังค้างตาม inventory. งาน performance อยู่กับ task อื่น.

### 2026-09-22 — Atomic Company provisioning receipt

เส้นทางเริ่มต้น Company ใน beta onboarding ตอนนี้สร้างกราฟขั้นต่ำใน Holding transaction เดียว: Company, สาขา, แผนกเริ่มต้น, policy, numbered staff levels 1–6, CEO position และการวางเจ้าของเป็น CEO. `company_provisioning` เก็บสถานะและ version พร้อม immutable started/completed events และรายละเอียดจำนวนกราฟ. Control-plane จะเปิด binding เป็น active หลังอ่าน receipt ที่เป็น active เท่านั้น จึง retry หลัง control write ล้มเหลวได้โดยไม่สร้าง Company หรือ access ซ้ำ.

`tests/social-beta.test.ts` ผ่าน 30 tests รวม injected write failure, isolated slots, waitlist, six levels, CEO placement, receipt events และ retry idempotency. เก็บหลักฐานไว้ที่ `evidence/2026-09-22-company-provisioning.txt`. งานนี้ปิดช่องว่าง initial provisioning ระดับ implementation/simulation; archive/relocation, Company business-owner, provider และ production acceptance ยังเป็น gate แยก. งาน performance ยังคงอยู่กับ task อื่น.


### 2026-09-22 — Shared mobile header and Company context

แก้ข้อความทับกันในหัว workspace โดยแยก breadcrumb ลงแถวที่สองบนหน้าจอไม่เกิน 1000px ขนาดปุ่มหลักอย่างน้อย 44×44 และ current page ย่อภายในพื้นที่ของตนเอง. Loading chooser ใช้ปุ่ม disabled ที่ขนาดสอดคล้องกัน. ระยะ focus/anchor ของ Organization ปรับตามความสูงหัวหน้า. แก้ชื่อ Company ใน Branch/Department context ให้ดึงบริษัทปัจจุบัน แทน fallback เป็นชื่อ Holding.

`demo-workspace-header.spec.ts` ผ่าน desktop/mobile (2 tests, 14.1s) ทั้ง light/dark และ 9 widths รวม 36 ชุดตรวจ geometry/hit target พร้อมเส้นทาง Search, breadcrumb Escape, Branch→Company, HR→Treasury→reload→HR และลิงก์ตรงคิว. Typecheck/scoped ESLint/diff check ผ่าน. Public Organization ตรวจจริงที่ 320/393/768px; ไม่เปลี่ยนบทบาทหรือ workspace ของผู้ใช้. หลักฐานอยู่ใน `docs/testing/evidence/2026-09-22-workspace-header-browser.txt` และ `...-records.json`.

แก้ข้อจำกัด shared header ที่รายงานไว้ในหลักฐาน Organization ก่อนหน้าแล้ว แต่ไม่ได้รับรอง responsive/accessibility ของทุกหน้า. เพิ่ม test นี้ใน `demo:verify:browser`. Goal Release 1–4 ยัง active; งานถัดไปคือ Company setup/provisioning และ safe organization changes ที่ยังค้างตาม capability inventory. งาน performance ยังคงอยู่กับ task อื่น.

Public recheck พบ session database connection timeout หนึ่งครั้ง (reference 3914836991) ตรวจ log แล้วกด Try again ผ่าน UI กลับมาโหลด Organization และ desktop header ได้; ไม่แก้ performance/runtime configuration และไม่ถือว่า root cause ของ timeout ปิดแล้ว.

### 2026-09-22 — Company setup acceptance

เพิ่ม Company setup acceptance ในหน้า Business readiness และ API จริง. ระบบคำนวณ checklist จาก Company identity, active branch, numbered staff levels, CEO position และ staff placement; ถ้ายังไม่ครบจะบล็อกการส่งพร้อมเหตุผล. เมื่อครบ ผู้มีอำนาจ Company ส่งคำขอได้ ผู้มีอำนาจอีกคนต้อง Accept หรือ Return for correction และเจ้าของสามารถ Revoke ภายหลังได้. ทุก transition ใช้ idempotency, optimistic version, current Company authority, independent-person guard, audit/outbox และ immutable event history. สถานะที่ส่งแล้วแสดงต่อใน Work และ Inbox พร้อม deep link เดิม.

`tests/organization.test.ts` ผ่าน 14 tests รวม blocker → submit → independent review → accept → revoke; typecheck และ scoped ESLint ผ่าน. `docs/testing/evidence/2026-09-22-company-business-acceptance.txt` เก็บผล service และ public readiness-page observation. งานนี้ปิดช่องว่าง Company business acceptance ของ Organization workflow ในระดับ implementation แต่ยังไม่ยกระดับ capability/provider/production gate และยังเหลือ initial provisioning กับ archive/relocation.

### 2026-09-22 — Organization archive and relocation

เพิ่ม action `archive` และ `relocate` เข้า reviewed organization workflow. Archive เก็บ node และ history ไว้ในสถานะ archived พร้อม reactivation ที่ผ่าน independent review และยังบล็อกถ้ามี child, staff, position, assignment หรือ warehouse ที่ใช้งานอยู่. Relocation ของ department รักษา route เดิมเป็น archived history แล้วสร้าง node ที่ destination branch ใน transaction เดียว พร้อมย้าย active engagements, employee placement, positions และ department-scoped assignments โดยไม่เขียนทับ route เก่า.

`tests/organization.test.ts` ผ่าน 14 tests รวม relocation พร้อม dependency handoff, archive/reactivate, blocked suspension, optimistic version, independent review และ immutable history. เก็บหลักฐานที่ `evidence/2026-09-22-organization-archive-relocation.txt`. งานนี้ปิดช่องว่าง archive/relocation ระดับ implementation/simulation; business-owner, provider, production และ downstream policy gates ยังแยกจากกัน. งาน performance ยังคงอยู่กับ task อื่น.
### 2026-09-22 — Platform migration runs

หน้า Platform operations แสดงจำนวน migration ที่ applied/pending แยกตาม Holding และมีคำสั่ง Run Holding migrations สำหรับ Platform administrator เท่านั้น. คำสั่งต้องมีเหตุผลและ Idempotency-Key, ใช้ PostgreSQL migration lock เดิม, บันทึก receipt ใน control plane พร้อม actor/assignment และ before/after counts, และ replay ด้วย payload เดิมจะคืนผลเดิม. Platform auditor ถูกปฏิเสธ และ payload คนละชุดที่ใช้ key เดิมถูกบล็อก.

`tests/platform-operations.test.ts` ผ่าน 5 tests; `bun run db:migrate:local` ผ่าน 6 databases/2 slots; typecheck และ scoped ESLint ผ่าน. หลักฐานอยู่ที่ `evidence/2026-09-22-platform-migration-runs.txt`. งาน provisioning commands, temporary access commands, off-machine/production RPO/RTO และ Company business acceptance ยังเป็น gate แยก. งาน performance ยังคงอยู่กับ task อื่น.

### 2026-09-23 — Platform Holding provisioning

เพิ่มคำสั่ง provisioning ที่ใช้งานจริงใน Platform operations สำหรับ resource slot ที่ operator provision ไว้แล้ว. Platform administrator เลือก slot, slug, ชื่อ Holding และเหตุผล ระบบตรวจ database/file resources ที่ resolve ได้จาก registry ก่อนเปิด binding; control-plane transaction เดียวบันทึก receipt, เปิด `holding_bindings` และเก็บ immutable `started`, `validated`, `activated` events. คำสั่งใช้ idempotency key แบบ replay-safe, ปฏิเสธ auditor, ตรวจ slug/resource ซ้ำ และหน้า Platform แสดงรายการ receipt กับ empty state เมื่อไม่มี slot ว่าง. Company setup และ invitation ยังคงเป็น handoff ถัดไป.

`tests/platform-operations.test.ts` ผ่าน 7 tests; `bun run db:migrate:local` ผ่าน 6 databases/2 slots; `bun run status:capabilities -- --check`, typecheck, scoped ESLint และ diff check ผ่าน. หน้า authenticated public Platform แสดง card `Provision a Holding` และแจ้งว่า local slots ทั้งสองถูกใช้งานแล้ว; local/public health HTTP 200 และ dev listener ยังอยู่ที่ 3100. หลักฐานอยู่ที่ `evidence/2026-09-23-platform-holding-provisioning.txt`. Physical database creation, temporary access commands, off-machine/production RPO/RTO, provider และ Company business acceptance ยังคงเป็น gates แยก. งาน performance ยังคงอยู่กับ task อื่น.
