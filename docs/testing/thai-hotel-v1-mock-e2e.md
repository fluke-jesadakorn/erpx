# Thai Hotel v1 mock E2E acceptance matrix

ปรับปรุง 2026-09-24 · scope เดียวกับ [`production-plan.md`](../production-plan.md)

เอกสารนี้เป็น acceptance matrix สำหรับชุดทดสอบจำลอง ไม่ใช่แผนส่งมอบฉบับที่สอง หลักฐานที่นี่พิสูจน์ได้เฉพาะ ERPX build และข้อมูลสังเคราะห์ใน browser simulation; ไม่เปลี่ยนสถานะ gate โรงแรมจริง, payroll ตามกฎหมาย, provider หรือ production release

## ขอบเขตการทดสอบ

- Holding โรงแรมคือ H002 (`riverside-lantern`) และทุกบุคคล โรงแรม ร้านอาหาร รายการเดินบัญชีและตัวเลขเป็นข้อมูลสมมติ
- ใช้ Next.js UI, API, authorization, domain services และ PostgreSQL ของ ERPX จริงในฐานข้อมูลที่สร้างแยกต่อ run
- ธนาคาร, payroll calculator และ AI เป็น test doubles ที่กำหนดจาก process ตอนเริ่ม simulation เท่านั้น; request และผู้ใช้ไม่มีทางเปิด test double
- Payroll mock ต้องมีป้ายกำกับชัดเจนว่าเป็นผลจำลอง ห้ามอ้างว่าเป็นภาษีหรือสูตรตามกฎหมายไทย และ production calculation ยังคงปฏิเสธ
- ทุก scenario ที่กำหนดต้องผ่านโดยไม่มี skip; หลัง reload ต้องอ่านค่าที่ persist ได้ และต้องตรวจความถูกต้องของ folio, stock, journal, payment หรือ source/version ตาม scenario
- เมื่อจบ run ต้องหยุด process ของ run นั้นและลบ PostgreSQL databases/files ของ run; ห้าม deploy, เปลี่ยน Tunnel หรือแก้ service 3101

## Acceptance matrix

สถานะ: **มีบางส่วน** หมายถึงมี browser path ใกล้เคียงแต่ยังไม่ผ่าน acceptance นี้ทั้งหมด; **ขาด** หมายถึงยังไม่มี browser scenario บน H002 ตามเกณฑ์; **ผ่าน mock E2E** ใช้ได้เฉพาะเมื่อมี evidence run ใหม่ที่ไม่ skip และสภาพแวดล้อมถูกทำลายแล้ว

| ID | Flow ที่ต้องผ่านบน H002 | หลักฐานใน repo ปัจจุบัน | ช่องว่างที่ต้องปิด | สถานะ |
| --- | --- | --- | --- | --- |
| HOTEL-01 | พนักงานสร้าง booking, เลือกห้อง/อัตรา/วัน, แม่บ้านทำความสะอาดและตรวจห้อง, front desk assign/check-in และเปิด folio | `tests/e2e/demo-hospitality-setup.spec.ts` เริ่มจาก stay ที่ fixture check-in ไว้แล้ว; PMS public booking เป็นคนละ fixture | ทำเส้นทาง staff booking → housekeeping → check-in ใน run เดียว และตรวจ rate/folio หลัง reload | มีบางส่วน |
| HOTEL-02 | ลงรายการ folio, checkout, Night Audit, ปิดวัน และแก้รายการที่ผิดด้วยผู้อนุมัติคนละคน | `demo-hospitality-setup.spec.ts` มี checkout/Night Audit และกรณีปิดก่อนเวลา; มี PMS service tests | เพิ่ม approved correction/reversal หลัง checkout และยืนยัน journal/folio source เดียว ไม่มียอดซ้ำ | มีบางส่วน |
| POS-01 | โต๊ะ/เมนู, kitchen ticket, consume และ waste พร้อมนับ stock และปิด cash shift | `demo-hospitality-setup.spec.ts` ส่งอาหารเข้าครัวและ consume สูตร; เปิด session เพื่อทดสอบ Night Audit blocker | ตรวจ stock delta และ waste ledger, รับเงินสด, ปิดกะและ reconcile cash/GL | มีบางส่วน |
| POS-02 | ชำระเงินสดหรือโอนที่ match statement fixture, charge to room, refund/reversal พร้อมอนุมัติแยกคน | `demo-hospitality-setup.spec.ts` ครอบคลุม room charge, pending transfer, bank match และการปฏิเสธ wrong-room/stale/revoked cashier | เพิ่ม tender/refund reversal ที่อนุมัติ, statement fixture เป็น test double แบบตั้งต้น, และ reconciliation หลัง checkout | มีบางส่วน |
| BUY-01 | PR → ขอและเปรียบเทียบ quotation → ผู้อนุมัติแยกคน → PO | `demo-purchase-request-rfq.spec.ts`, `demo-purchase-rfq.spec.ts` และหลักฐาน 2026-09-22 ใช้ H001 บริษัทจำลองทั่วไป | ย้าย fixture และ browser flow ไป H002; ตรวจผู้ขอ/ผู้ซื้อ/ผู้อนุมัติ, ราคาที่เปรียบเทียบและการ replay | มีบางส่วน |
| BUY-02 | รับของบางส่วนและครบ, invoice match, payment export และ match กับ statement fixture | H001 purchase-to-payment และ `demo-controlled-payment.spec.ts` เป็น reference; ไม่ใช่โรงแรม H002 | เพิ่ม partial/full receipt, variance/rejection, invoice reconciliation, export และ bank match ใน H002 พร้อม stock/AP/GL assertions | มีบางส่วน |
| HR-01 | สร้าง/แก้ employee, engagement, หัวหน้าและ access; revoke แล้วข้อมูล/command ถูกปฏิเสธ | People lifecycle และ IAM มี service tests; hospitality actors เป็นเพียง role fixtures | ทำ browser lifecycle ใน H002 และยืนยัน scope/สิทธิ์หลัง reload, revocation และ cross-company request | ขาด |
| HR-02 | roster, time, leave และ OT ผ่านหัวหน้า/HR ก่อน handoff ไป payroll | `demo-overtime-to-payroll.spec.ts` ใช้ H001; `leave-approval.spec.ts` ใช้ environment อื่น | เพิ่มบัญชี HR/พนักงานใน H002 และทดสอบ handoff ทุกชนิดพร้อมสิทธิ์ที่จำกัด | มีบางส่วน |
| PAY-01 | Mock run ครอบคลุม monthly/daily/hourly, foreign employee prorate, bonus, service charge, OT, YTD, correction และ final pay | `demo-payroll-runs.spec.ts` กับ `demo-overtime-to-payroll.spec.ts` ทดสอบ H001 gross preview; `PAYROLL_ENGINE_UNAVAILABLE` ยังปิด statutory calculation จริง | เพิ่ม calculator test double แบบ deterministic ซึ่งเลือกได้จาก process startup เท่านั้น, seeded expected fixtures ครบกลุ่ม, mock labels และ negative production assertion | ขาด |
| PAY-02 | ทบทวนและอนุมัติคนละคน, payslip/review export, GL posting, payment export และ statement reconcile | `docs/testing/evidence/2026-09-23-payroll-gl-posting.txt` ยืนยัน service/Finance posting และ sandbox payment บนชุดทดสอบทั่วไป | ย้าย full browser flow ไป H002; ตรวจ persisted result, payslip, balanced GL, export checksum และ bank statement match | มีบางส่วน |
| AI-01 | AI ร่าง PR จาก source/version ที่มีสิทธิ์; คนอ่านและยืนยัน; source เปลี่ยนแล้ว proposal stale; ใช้มือได้เมื่อ provider ล้ม | `tests/request-ai.test.ts` ตรวจ service/worker; ยังไม่พบ H002 browser E2E | เพิ่ม provider mock แบบ deterministic เฉพาะ simulation, UI review/human confirmation, source-version invalidation และ manual fallback | ขาด |
| AI-02 | สรุปก่อนปิดวันจาก record ที่เห็นได้ พร้อม source/version; เปลี่ยน record/provider แล้วคำตอบเก่าหมดอายุ; failure ทำงานต่อด้วยมือ | `workspace-assistant` มี source revalidation แต่ไม่มี E2E สำหรับ hotel day-close และยังไม่มี day-close-specific evidence | เพิ่ม day-close view/summary ที่จำกัดแหล่งข้อมูล, deterministic provider/failure cases, citations และ manual close path | ขาด |
| AUTH-01 | ปฏิเสธ self-review, revoked assignment, cross-company access และ stale version ก่อนเขียนข้อมูล | Hospitality POS มี wrong-room, stale และ revoked-cashier cases; module tests มี coverage แยกส่วน | ทำ matrix เดียวให้ครอบคลุม PMS, POS, purchasing, HR/payroll และ AI บน H002; ตรวจยอดก่อน/หลังทุก rejection | มีบางส่วน |
| MONEY-01 | คำสั่งซ้ำ, worker retry/crash และ replay ไม่สร้างเงิน, stock, folio หรือ journal ซ้ำ | Domain tests ทดสอบ idempotency แยก service; browser POS ตรวจ source/journal เดียวสำหรับ room charge | เพิ่ม browser failure/retry สำหรับ transfer, receipt, invoice/payment, payroll posting และ AI jobs; assert ledger/stock deltas หลัง replay | มีบางส่วน |
| UI-01 | หน้าทำงานสำคัญใช้ได้ desktop และ mobile ตามบทบาท รวม login, error/recovery และ persistence หลัง reload | Hospitality browser test เปิด cashier ที่ 390px; Playwright มี desktop/mobile projects | รัน scope ทั้งหมดบน desktop และหน้าที่ใช้หน้างานบน mobile; เก็บผลจริงต่อ project โดยไม่ใช้ skip | มีบางส่วน |

## ชุดรันทดสอบและหลักฐาน

ชุดรวมสุดท้ายต้องเริ่มจาก source checkout ปัจจุบัน, Bun 1.2.17, Node.js 24.20.0 และ isolated PostgreSQL browser simulation ที่พอร์ต 3102 ว่าง หลังเตรียมต้องบันทึก test run ID, commit/build fingerprint, รายชื่อ scenario, จำนวน pass/fail/skip แยก desktop/mobile, digest ของ Playwright report และผลยืนยัน cleanup ของ run นั้น

ผลการรันเก่าที่อ้างข้างต้นเป็น baseline เท่านั้น ห้ามรวมเป็นจำนวนผ่านของ suite ใหม่ เมื่อ suite ใหม่พร้อม ต้องบันทึก summary ใต้ `docs/testing/evidence/` และอ้างจาก `docs/production-plan.md` โดยคง status ของ release/business/provider gates ตามเดิมจนผู้ตรวจที่เกี่ยวข้องรับรองหลักฐานจริง

## เงื่อนไขจบ mock E2E

ปิด mock suite ได้เมื่อทุกแถวผ่านครบ, ไม่มี skipped tests, reload และ database assertions ผ่าน, report ระบุ source/build เดียวกัน, mock adapters ไม่สามารถเลือกจาก request หรือ production config และ run ถูกทำลายแล้วเท่านั้น ความสำเร็จนี้ไม่ใช่การรับรองโรงแรมจริง สูตร payroll ตามกฎหมายไทย การโอนเงินจริง หรือการเปิด production
