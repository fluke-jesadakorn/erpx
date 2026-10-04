# Thai Hotel v1 mock E2E acceptance matrix

> Recorded report — findings, counts, gaps and hostname observations below belong to the original run. Current workflows: [owner catalogue](../contracts/next/catalogue.md); latest recorded engineering/provider/release boundaries: [verification inventory](../status/capability-delivery-matrix.md). This report is retained evidence, not an active plan.

ปรับปรุง 2026-09-29 · scope เดียวกับ [`production-plan.md`](../production-plan.md)

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
| HOTEL-01 | พนักงานสร้าง booking, เลือกห้อง/อัตรา/วัน, แม่บ้านทำความสะอาดและตรวจห้อง, front desk assign/check-in และเปิด folio | [`tests/e2e/thai-hotel-v1-hotel.spec.ts`](../../tests/e2e/thai-hotel-v1-hotel.spec.ts): staff booking → housekeeping/independent inspection → assign/check-in; reload และตรวจ rate, stay, room, guest, folio | UAT กับผู้จัดการโรงแรมและข้อมูลโรงแรมจริง; ตรวจ production configuration | ผ่าน mock E2E |
| HOTEL-02 | ลงรายการ folio, checkout, Night Audit, ปิดวัน และแก้รายการที่ผิดด้วยผู้อนุมัติคนละคน | [`tests/e2e/thai-hotel-v1-hotel.spec.ts`](../../tests/e2e/thai-hotel-v1-hotel.spec.ts): ชำระค่าพัก → checkout → บันทึกรายการผิด → independent correction/reversal → ปิด folio/กะ → Night Audit; ตรวจ blocker, ผู้ขอ/ผู้อนุมัติ, source/reversal, invoice และ journal balance | UAT กับผู้จัดการโรงแรม/Finance บนข้อมูลจริงและ build/config สำหรับ release | ผ่าน mock E2E |
| POS-01 | โต๊ะ/เมนู, kitchen ticket, consume และ waste พร้อมนับ stock และปิด cash shift | [`2026-09-28 H002 flow evidence`](evidence/2026-09-28-thai-hotel-v1-flows.txt): เลือก T01 และเมนู; kitchen consume/waste ตรงกับ stock ledger; รับเงินสดและปิดกะ variance เป็นศูนย์ | ผ่าน mock flow; การตรวจรับกับผู้จัดการร้าน/Finance และข้อมูลจริงยังแยกอยู่ใน production gate | ผ่าน mock E2E |
| POS-02 | ชำระเงินสดหรือโอนที่ match statement fixture, charge to room, refund/reversal พร้อมอนุมัติแยกคน | [หลักฐานเพิ่ม 2026-09-29](evidence/2026-09-29-thai-hotel-v1-acceptance.txt): transfer เทียบ bank row, room charge → รับชำระ → checkout, independent refund, wrong-room/stale rejection และ replay charge โดย folio, posting และ stock movement ไม่ซ้ำ | ผ่าน mock flow; โรงแรม/Finance จริงและ payment provider acceptance ยังเป็น release gate | ผ่าน mock E2E |
| BUY-01 | PR → ขอและเปรียบเทียบ quotation → ผู้อนุมัติแยกคน → PO | [`2026-09-28 H002 flow evidence`](evidence/2026-09-28-thai-hotel-v1-flows.txt): H002 request ผ่าน manager/Finance; Purchasing เทียบราคาและผู้ตรวจแยกอนุมัติ PO สองใบ | ผ่าน mock flow; การยืนยันกับผู้ซื้อ/ผู้อนุมัติของโรงแรมจริงยังเป็น business acceptance | ผ่าน mock E2E |
| BUY-02 | รับของบางส่วนและครบ, invoice match, payment export และ match กับ statement fixture | [`2026-09-28 H002 flow evidence`](evidence/2026-09-28-thai-hotel-v1-flows.txt): partial/full receipt, goods GL, consolidated invoice, payment export และ synthetic bank match/settlement | ผ่าน mock flow; bank/provider และ Finance acceptance จริงยังคงแยกจาก simulation | ผ่าน mock E2E |
| HR-01 | สร้าง/แก้ employee, engagement, หัวหน้าและ access; revoke แล้วข้อมูล/command ถูกปฏิเสธ | [`tests/e2e/thai-hotel-v1-hr-lifecycle.spec.ts`](../../tests/e2e/thai-hotel-v1-hr-lifecycle.spec.ts) และ [หลักฐานเพิ่ม 2026-09-29](evidence/2026-09-29-thai-hotel-v1-acceptance.txt): employee activate/login, cross-holding read ถูกปฏิเสธ, revoke access แล้ว read ถูกปฏิเสธหลัง reload ทั้ง desktop/mobile | ผ่าน mock flow; identity/HR acceptance ของโรงแรมจริงยังคงแยกอยู่ | ผ่าน mock E2E |
| HR-02 | roster, time, leave และ OT ผ่านหัวหน้า/HR ก่อน handoff ไป payroll | [`2026-09-28 H002 flow evidence`](evidence/2026-09-28-thai-hotel-v1-flows.txt): manager/HR ตรวจ OT, time, roster-calculated leave และ leave handoff | ผ่าน mock flow; HR lifecycle/employee setup อยู่ใน HR-01 แยกต่างหาก | ผ่าน mock E2E |
| PAY-01 | Mock run ครอบคลุม monthly/daily/hourly, foreign employee prorate, bonus, service charge, OT, YTD, correction และ final pay | [`2026-09-28 H002 flow evidence`](evidence/2026-09-28-thai-hotel-v1-flows.txt): normal mock ครอบคลุม worker 4 แบบ, OT, bonus/service charge, YTD และ correction; final-pay mock ถูกเตรียมจาก retirement clearance และคำนวณแบบ person-scoped บน H002 | Mock flow ครบ; ตัวคำนวณและผลตามกฎหมายไทยยังต้องผ่าน Payroll Engineering/ผู้เชี่ยวชาญใน release gate | ผ่าน mock E2E |
| PAY-02 | ทบทวนและอนุมัติคนละคน, payslip/review export, GL posting, payment export และ statement reconcile | [หลักฐาน H002](evidence/2026-09-29-thai-hotel-v1-acceptance.txt): independent review/correction, review export, GL, payment export, synthetic bank confirmation และ replay คง payment batch เดียว; UI ยืนยันว่า live employee payslip ยังถูกล็อก | mock acceptance ผ่านในขอบเขต review export และ lock behavior; ไม่ได้ออกหรือส่ง payslip จริง. Thai statutory result และ restricted employee delivery ต้องผ่านผู้เชี่ยวชาญ/HR ก่อนเปิด | ผ่าน mock E2E |
| AI-01 | AI ร่าง PR จาก source/version ที่มีสิทธิ์; คนอ่านและยืนยัน; source เปลี่ยนแล้ว proposal stale; ใช้มือได้เมื่อ provider ล้ม | [`2026-09-28 H002 flow evidence`](evidence/2026-09-28-thai-hotel-v1-flows.txt): source/version proposal ถูกพนักงานตรวจและบันทึก, stale หลังแก้ source, manual fallback เมื่อปิด AI | ผ่าน mock flow; provider จริงและ Company acceptance ยังแยกอยู่ | ผ่าน mock E2E |
| AI-02 | สรุปก่อนปิดวันจาก record ที่เห็นได้ พร้อม source/version; เปลี่ยน record/provider แล้วคำตอบเก่าหมดอายุ; failure ทำงานต่อด้วยมือ | [หลักฐาน 2026-09-28 และ 2026-09-29](evidence/2026-09-29-thai-hotel-v1-acceptance.txt): คำตอบอ้าง hotel source/version และหมดอายุหลังเปลี่ยน source; เมื่อปิด AI พนักงานปิด Night Audit ด้วยมือ แล้ว worker จบงานและ replay โดยไม่สร้าง night record ซ้ำ | ผ่าน mock flow; provider จริงและ Company acceptance ยังแยกอยู่ | ผ่าน mock E2E |
| AUTH-01 | ปฏิเสธ self-review, revoked assignment, cross-company access และ stale version ก่อนเขียนข้อมูล | [หลักฐาน H002 รวม](evidence/2026-09-29-thai-hotel-v1-acceptance.txt): PMS worker ถูก block หลัง revoke แล้วผู้มีสิทธิ์ resolve; POS ปฏิเสธ wrong-room/stale; Purchasing/Payroll ปฏิเสธ self-approval; HR ปฏิเสธ cross-holding และ revoked access พร้อมตรวจไม่มี business write | ผ่าน mock matrix ตาม role/holding ที่ทดสอบ; การอนุมัติจริงของเจ้าของธุรกิจยังคงเป็น release gate | ผ่าน mock E2E |
| MONEY-01 | คำสั่งซ้ำและ worker failure/retry/replay ไม่สร้างเงิน, stock, folio หรือ journal ซ้ำ | [หลักฐาน H002 รวม](evidence/2026-09-29-thai-hotel-v1-acceptance.txt): POS charge/payment replay, supplier receipt/settlement replay, payroll bank/payment replay, Night Audit worker blocked → resolve → retry; นับ source, folio, stock movement, invoice และ journal หลัง replay | ผ่าน mock idempotency/recovery flows; ไม่ใช่การทดสอบการโอนเงินจริงหรือ restore production | ผ่าน mock E2E |
| UI-01 | หน้าทำงานสำคัญใช้ได้ desktop และ mobile ตามบทบาท รวม login, error/recovery และ persistence หลัง reload | HOTEL-01/02, POS-01/02, Procurement, HR-01, Payroll, AI และ Night Audit recovery ผ่าน desktop/mobile ตาม [หลักฐาน 2026-09-29](evidence/2026-09-29-thai-hotel-v1-acceptance.txt); flow มี reload/persist และหน้า access denied/recovery | ผ่าน mock UI coverage; ยังต้องตรวจ UX กับพนักงานและอุปกรณ์จริงของโรงแรม | ผ่าน mock E2E |

## ชุดรันทดสอบและหลักฐาน

ชุดรวมสุดท้ายต้องเริ่มจาก source checkout ปัจจุบัน, Bun 1.2.17, Node.js 24.20.0 และ isolated PostgreSQL browser simulation ที่พอร์ต 3102 ว่าง หลังเตรียมต้องบันทึก test run ID, commit/build fingerprint, รายชื่อ scenario, จำนวน pass/fail/skip แยก desktop/mobile, digest ของ Playwright report และผลยืนยัน cleanup ของ run นั้น

การปิด mock matrix สะสมจาก isolated H002 runs โดยบันทึก run ID, viewport, report digest, source fingerprint และ cleanup ใน evidence ใต้ `docs/testing/evidence/`; เป็นการยืนยัน flow บน mock เท่านั้น ไม่ยกระดับ release/business/provider gates จนผู้ตรวจที่เกี่ยวข้องรับรองหลักฐานจริง

## หลักฐาน H002 — 2026-09-29

[หลักฐาน HOTEL-01/02](evidence/2026-09-28-thai-hotel-v1-hotel.txt) และ [หลักฐาน flow เดิม](evidence/2026-09-28-thai-hotel-v1-flows.txt) บันทึก baseline; [หลักฐานปิด acceptance เพิ่มเติม](evidence/2026-09-29-thai-hotel-v1-acceptance.txt) บันทึก run IDs, source fingerprints, report digests และ cleanup สำหรับ POS, HR, Payroll, AI, AUTH, MONEY และ responsive/recovery UI. รวมผ่าน mock E2E **15 จาก 15 rows**; เพิ่มจาก 8 เป็น 15 (**+7 acceptance rows**). PAY-02 ผ่านเฉพาะ review export และตรวจยืนยันว่า live payslip ยังล็อกอยู่; ไม่มีการออกหรือส่งสลิปให้พนักงาน. ไม่ใช่ UAT โรงแรมจริงและไม่ปิด gate release; capability inventory ยังคง 68/187.

## เงื่อนไขจบ mock E2E

ปิด mock suite เมื่อทุกแถวผ่านโดยไม่มี skip, reload และ database assertions ผ่าน, มี report/source fingerprint และทำลาย simulation run แล้ว. ณ 2026-09-29 ครบ **15/15 mock E2E rows** ตามหลักฐานที่ลิงก์ไว้. PAY-02 ไม่รวมการออก payslip จริง; ผลทั้งหมดไม่ใช่การรับรองโรงแรมจริง สูตร payroll ตามกฎหมายไทย การโอนเงินจริง provider production หรือการเปิด production
