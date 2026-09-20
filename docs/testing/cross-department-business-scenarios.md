# ERPX — ตรวจสถานการณ์ธุรกิจข้ามแผนกและผลลัพธ์ต่อลูกค้า

> สถานะหลัง audit: เริ่ม implement ตามแผนแล้ว ดู [progress register](cross-department-implementation-progress.md) และ [paid-return/refund simulation S04](return-refund-simulation.md) สำหรับสิ่งที่เปลี่ยนและผลทดสอบใหม่ ตารางด้านล่างคงเป็น baseline ณ เวลาตรวจ ไม่ใช่ production sign-off

ฐานตรวจ: workspace ปัจจุบันวันที่ 2026-09-19 UTC / 2026-09-20 Asia/Bangkok, HEAD `00b8f922` รวมไฟล์ที่ยังไม่ commit; ไม่ใช่การรับรอง production หรือ UAT โดยลูกค้าจริง รายงานนี้ตรวจและเสนอช่องว่าง ไม่เปลี่ยน API, schema หรือ business behavior

## 1. ข้อสรุปสำหรับเจ้าของธุรกิจ

**ระบบยังไม่ครบวงจรสำหรับทุกธุรกิจและทุกแผนก** มีแกนธุรกรรมจริงใน Sales/Procurement/Inventory/Finance, งานพนักงาน, POS, PMS และ Legal แต่การมีหลายโมดูลไม่ได้แปลว่ามีการส่งต่อครบทุกคู่ โดยเฉพาะก่อนขาย หลังขาย การคืนเงินหลังชำระครบ งานผลิต และ POS ลงห้องพัก

- เริ่มทดลองแบบควบคุมได้กับงานเอกสารซื้อขายและบัญชี งานขออนุมัติพนักงาน และงาน Legal ภายใน หลังตั้งสิทธิ์ ผู้รับงาน และบัญชีให้ครบ
- POS และ PMS มี domain workflow และ automated scenarios จำนวนมาก แต่ Acme บน public development ยังแสดง POS **Not configured**, PMS **Set up your first hotel** และหน้าลูกค้า **Online reservations are not open yet** ณ การตรวจรอบนี้ จึงยังรับรองการใช้งานหน้าร้าน/โรงแรมจริงไม่ได้
- CRM มีทะเบียนลูกค้าและทางไปใบเสนอราคา แต่ยังไม่ใช่ lead/opportunity pipeline; Support, Field Service, Manufacturing, Planning และ Quality ยังไม่มี owner-specific flow ตามทะเบียนและการตรวจเส้นทาง implementation ปัจจุบัน
- Legal มี implementation มากกว่าเอกสารสถานะวันที่ 7 กันยายนอย่างชัดเจน รวมการอนุมัติ ลายมือชื่อเชิงหลักฐาน หน้าที่ตามสัญญา และส่งค่าที่ปรึกษาไปคำขอซื้อ แต่ยังต้อง UAT ผู้ใช้/ผู้ลงนามจริง และการเชื่อมสัญญากับการส่งมอบเชิงพาณิชย์
- ไม่ใช้ตัวเลข capability เป็นเปอร์เซ็นต์ความพร้อมธุรกิจ: registry มี **32 modules / 183 capabilities** รวม Legal 5 รายการ; non-Legal 178 รายการระบุ 41 Flow accepted, 5 Sandbox verified, 1 Live verified, 46 Implemented, 85 Deferred สถานะเหล่านี้เป็นคำประกาศในทะเบียน ต้องอ่านคู่กับหลักฐานด้านล่าง

### ความหมายของผลประเมิน

| ผล scenario | เกณฑ์ |
|---|---|
| ครบตามเกณฑ์จำลอง | ทุกขั้นจำเป็นและผลต่อลูกค้าในขอบเขตนั้นผ่านหลักฐานที่กำหนด ไม่มีช่องว่างสำคัญ ไม่เท่ากับ production-ready |
| ได้บางส่วน | มีธุรกรรมจริงอย่างน้อยหนึ่งช่วง แต่ยังมีขั้นขาด งานทำมือ หรือ acceptance ที่จำเป็นยังไม่ผ่าน |
| ยังไม่มี | ยังไม่มีเส้นทางธุรกิจต้นน้ำถึงปลายน้ำตาม scenario; อาจมีองค์ประกอบทั่วไปให้ใช้บันทึก |
| ยังยืนยันไม่ได้ | สถานะ runtime/provider/UI หรือหลักฐานไม่พอจะตัดสิน ไม่ถือว่า failed business behavior โดยอัตโนมัติ |

**ยังไม่มี scenario ใหญ่ด้านล่างที่ประกาศว่าครบทุกเกณฑ์ลูกค้า** การทดสอบย่อยที่ผ่านรับรองเฉพาะสิ่งที่ assertion ตรวจ ไม่ขยายผลไปถึงการส่งอีเมลจริง การรับมอบของจริง หรือการยืนยันของลูกค้า

## 2. วิธีตรวจ หลักฐาน และขอบเขต

ตรวจ registry → live destination/UI → API → owner service → migration → tests โดยให้โค้ดปัจจุบันและผลรันใหม่สำคัญกว่าเอกสารเก่า ไม่ถือ generic record CRUD, ลิงก์อ้างอิง, เมนู หรือหน้า preview เป็น workflow สำเร็จ

### ดัชนีหลักฐาน

| รหัส | หลักฐานและสิ่งที่ใช้ยืนยัน |
|---|---|
| E01 | [Capability registry](../../src/data/module-registry.json), [delivery](../../src/data/capability-delivery.ts), [live destinations](../../src/data/module-registry.ts): inventory/เส้นทางและช่องว่างที่ประกาศ ไม่ใช่ผล UAT |
| E02 | [Commerce service](../../src/server/commerce.ts), [API](../../src/app/api/v1/holdings/[holdingSlug]/commerce/[...path]/route.ts), [baseline schema](../../migrations/postgres/holding-write/0001_baseline.sql), [Flow tests](../../tests/flow-routing.test.ts): quote/order, partial fulfilment, GL, invoices, payment, credit limit, cancellation |
| E03 | [Flow routing](../../src/server/flow-routing.ts), [Flow work](../../src/server/flow-work.ts), [handoff contract](../plan/2026-09-10-flow-handoffs.md): named owner, eligibility, revision, My Work และการแยกอนุมัติ/GL/ชำระ |
| E04 | [Returns test](../../tests/commerce-returns.test.ts), [return schema](../../migrations/postgres/holding-write/0201_commerce_returns.sql), [payment correction test](../../tests/finance-payment-corrections.test.ts), [Finance service](../../src/server/finance.ts): supplier return และแก้รับเงินผิด; ไม่ใช่หลักฐาน customer paid-refund |
| E05 | [POS service](../../src/server/pos.ts), [POS API](../../src/app/api/v1/holdings/[holdingSlug]/pos/[...path]/route.ts), [POS schema](../../migrations/postgres/holding-write/0030_pos.sql), [POS tests](../../tests/pos.test.ts), [contract](../contracts/next/pos.md) |
| E06 | [PMS service](../../src/server/pms.ts), [PMS schema](../../migrations/postgres/holding-write/0031_pms.sql), [PMS tests](../../tests/pms.test.ts), [booking tests](../../tests/pms-booking.test.ts), [contract/API](../contracts/next/pms.md) |
| E07 | [Room-charge contract](../../src/lib/pms-room-charge.ts), [POS payment methods](../../src/lib/pos.ts): prepareRoomCharge มีผู้เรียกใน tests แต่ไม่พบใน production service; payment methods ไม่มี room charge |
| E08 | [Staff tasks](../../src/server/staff-tasks.ts), [employee requests](../../src/server/employee-requests.ts), [staff tests](../../tests/staff-work.test.ts), [expense tests](../../tests/expense-v2.test.ts), [self-service tests](../../tests/self-service-journeys.test.ts): internal requests, projects/tasks, approvals, purchase-order handoff |
| E09 | [People lifecycle tests](../../tests/people-lifecycle.test.ts), [roster tests](../../tests/people-roster.test.ts), [payroll tests](../../tests/payroll-core.test.ts), [payroll service](../../src/server/people-payroll.ts): cutoff/reassignment, shifts, payroll preview; production mode ปฏิเสธด้วย PAYROLL_ENGINE_UNAVAILABLE |
| E10 | [Transfers](../../src/server/inventory-transfers.ts), [counts](../../src/server/inventory-counts.ts), [ledger](../../src/server/ledger.ts), [opening tests](../../tests/finance-opening-balances.test.ts), [Executive metrics](../../src/server/executive/metrics.ts): same-company stock/finance/reporting; ไม่ใช่ group consolidation acceptance |
| E11 | [Legal contract](../contracts/next/legal.md), [Legal tests](../../tests/legal.test.ts), [deadline tests](../../tests/legal-deadlines.test.ts), [Work/Calendar adapter](../../src/server/legal/integrations.ts), [source evidence](../../src/server/legal/sources.ts): native confidential workflows; schema 0059–0071 ตาม contract |
| E12 | [Billing service](../../src/server/billing.ts), [Billing tests](../../tests/billing.test.ts), [provider test](../../tests/billing-stripe-sandbox.test.ts), [billing contract](../contracts/next/billing.md): hosted billing sandbox; ต้องแยกจาก POS Stripe |
| E13 | [Assistant](../../src/server/workspace-assistant.ts), [assistant tests](../../tests/workspace-assistant.test.ts), [social](../../src/server/social.ts), [executive tests](../../tests/executive-canvas.test.ts): งานสนับสนุน/การเห็นข้อมูล ไม่ใช่ agent อนุมัติหรือชำระแทนคน |
| E14 | [Brand tests](../../tests/branding.test.ts), [brand contract](../adr/ADR-027-workspace-and-document-branding.md), [native recovery tests](../../tests/native-recovery.test.ts): issuer/brand และ recovery เฉพาะขอบเขต fixture |
| E15 | [POS formatting](../../src/lib/pos.ts), [application currency](../../src/lib/localization.ts): formatPosMoney ใช้ APP_CURRENCY=THB แทนรับสกุลเงินเอกสาร; public Commerce แสดง USD ขณะที่ POS แสดง THB — ต้อง UAT การแสดงผลเมื่อบริษัทใช้ USD ก่อนขาย |

### ผลตรวจที่รันในรอบนี้

<!-- AUDIT-RUN-RESULTS -->
| รายการ | ผลใหม่ใน audit นี้ |
|---|---|
| Toolchain สำหรับ integration | Node 24.20.0 จาก `.runtime/node/node-v24.20.0-darwin-arm64/bin`; Bun 1.2.17 |
| คำสั่ง | `bun run test:integration` โดยเติม pinned Node ลง PATH และโหลดเฉพาะ `ERPX_TEST_ADMIN_URL` จาก `.env.local` ด้วย Python subprocess environment; ไม่มีการพิมพ์ URL/รหัสผ่าน |
| เวลาเริ่ม/ระยะเวลา | 2026-09-19 23:50:51 Asia/Bangkok; 506.88 วินาที; process exit 0 |
| ผลรวม | **78 test files passed, 2 files skipped; 719 tests passed, 4 tests skipped; 0 failures** |
| ชุดหลักที่อยู่ในการรัน | flow-routing, commerce-returns, finance-payment-corrections, finance-opening-balances, expense-v2, staff-work, people-lifecycle, people-roster, payroll-core, pos, pms, pms-booking, legal, legal-deadlines, billing, workspace-assistant, executive-canvas และ suites อื่นตาม `tests/*.test.ts` |
| 4 tests ที่ skipped | actual Stripe Sandbox collection; real UCS write/read-back; real GPO verification; Redis proxy-failure/reconnect case (ตาม `skipIf` ของไฟล์ทดสอบ) ไม่เปิด flags หรือส่งธุรกรรมออกบริการจริงในรอบนี้ |
| Browser acceptance ที่ทำใหม่ | อ่าน 6 หน้าด้านล่างผ่าน authenticated session ที่มีอยู่ และกด Waiting บน Work เห็นผลกรองจริง; ไม่มีการสร้างยอดขาย/เปลี่ยนสิทธิ์/ส่งข้อความ/ลงนาม |
| ไม่ได้รันใน audit นี้ | Playwright transactional E2E, HTTP scripts ที่เขียนลง development, real email/Stripe/POS peripherals, legal Python certificate/OCR fixtures; หลักฐานเก่าใน contracts ไม่ถือเป็น fresh rerun |
| Availability หลังรีสตาร์ต dev ด้วย pinned Node | local `3100/api/health` **200**, public `/api/health` **200**, public `/book/acme` **200**, installed production `3101/api/health` **200**; authenticated public Work และ Waiting เปิดสำเร็จ |
| Persistent dev | launcher PID 56138 / listener PID 56148 ณ ตรวจ, cwd workspace นี้, executable เป็น pinned Node 24.20.0; detached log/PID อยู่ `.scratch/dev/cross-department-dev.*`; ไม่มีการเปลี่ยน production/Tunnel |

Raw integration logs อยู่ ignored `.scratch/cross-department-audit/integration-configured.log` (ผลรันใหม่) และ `integration.log` (setup failure ครั้งแรก) ตัวเลขข้างบนคือหลักฐานถาวรในรายงาน; ไม่ต้องมีไฟล์ scratch เพื่ออ่านข้อสรุป
<!-- /AUDIT-RUN-RESULTS -->

ทดสอบด้วย disposable `erpx_test_*` databases ตาม guard ใน [test runtime](../../tests/runtime-env.ts) ใช้เฉพาะบัญชี `erpx_test_admin` ที่ไม่เป็น superuser ตรวจ ownership ก่อนลบ และไม่เปิดสิทธิ์ข้ามฐานข้อมูลโปรเจกต์ การรันแรกไม่มี password ใน environment จึงล้มเหลวที่ setup (78 files failed, 723 tests skipped); ไม่ใช่ 723 business tests ล้มเหลว โหลดเฉพาะ ERPX_TEST_ADMIN_URL ที่ตั้งไว้แล้วจึงรันใหม่โดยไม่แสดงค่า secret

ข้อสังเกต reproducibility: runbook macOS กล่าวถึง test port 5434 แต่ runtime ปัจจุบันบังคับ 5432; ใช้ guard ปัจจุบัน ไม่แก้ให้ชี้ฐาน production ต้องปรับเอกสาร tooling ในงานถัดไป

### Public browser — อ่านผ่าน session ที่มีอยู่ ไม่สร้างธุรกรรม

| หน้า | สิ่งที่เห็นรอบนี้ | ข้อจำกัด |
|---|---|---|
| `/holdings/acme/work` | My actions/Waiting/Team/Completed และหลายประเภทงาน; กด Waiting แล้วพบ expense หนึ่งรายการขึ้น **The primary staff member is no longer eligible. Reassign with a reason.** | มี handoff ที่ติดการตั้งผู้รับงานจริงในข้อมูลปัจจุบัน → G20; ตรวจด้วย holding owner คนเดียว ไม่ได้ยืนยันทุกบทบาท/notification |
| `/holdings/acme/commerce` | Sales orders 4 รายการสถานะ fulfilled พร้อม required date, customer และ USD | อ่านรายการ ไม่ได้สร้าง quote → delivery → receipt ใหม่บน public |
| `/holdings/acme/pos` | Not configured, account/register setup, ไม่มี open cash session; สินค้าแสดง THB | ยังขายจริงไม่ได้; การเลือก register ไม่เท่ากับผ่าน setup |
| `/holdings/acme/pms` | Set up your first hotel; ยังไม่มี hotel ให้เลือก | ยังไม่มี property/rooms/rates ให้ทำ browser stay journey |
| `/book/acme` | Online reservations are not open yet; pay at hotel | ลูกค้ายังจอง Acme ไม่ได้ ไม่มีหลักฐาน online payment |
| `/holdings/acme/legal` | request จำลองหนึ่งรายการ confidential/draft, registers และ reports/knowledge tools | ไม่ได้ลงนามจริง ส่ง invitation หรือรับรองผลกฎหมาย |

ไม่มี login blocker ใน session นี้ แต่มี setup blockers ข้างต้น HTTP 200 ยืนยันการเปิดหน้าเท่านั้น ไม่รับรอง workflow

## 3. Scenario และจุดส่งต่อที่ต้องผ่าน

### กติกาชุดจำลองร่วม

- ใช้บริษัทสมมติ A สาขากรุงเทพฯ/ภูเก็ต และบริษัท B อีกแห่งเพื่อทดสอบ isolation; ลูกค้า C001 และ vendor V001 เป็นข้อมูลสมมติ ใช้ THB ใน fixture ชุดใหม่ เว้นแต่ทดสอบ currency โดยเฉพาะ
- เริ่มจากงวดเปิด บัญชี GL/ภาษี/คลังพร้อม และผู้ปฏิบัติแยกคนตามหน้าที่ ไม่มีการใช้ owner คนเดียวแทน independent approvers
- เวลาทั้งหมด Asia/Bangkok; SLA ด้านล่างเป็น **เกณฑ์ UAT ที่เสนอ** ไม่ใช่คำกล่าวว่าระบบตั้งเวลา/escalation นี้ได้แล้ว ให้ผู้ใช้ธุรกิจยืนยันก่อน pilot
- ทุกลูกศรหมายถึงเกณฑ์ส่งมอบ: ผู้ส่งส่ง record/version+หลักฐาน → ผู้รับมีสิทธิ์และเห็น next action/วันครบกำหนด → หัวหน้าแผนกผู้รับตามเมื่อค้าง → เจ้าของความสัมพันธ์ลูกค้าแจ้งผล หากไม่มี automation ให้ลงเป็นช่องว่าง ไม่ถือว่าการแชทแทน workflow ทำให้ผ่าน
- A/B/C ในแต่ละรายการเป็น acceptance cases ที่กำหนด; การดำเนินการรอบนี้อาศัย existing automated tests และ browser read-only ตามตารางหลักฐาน ไม่ได้อ้างว่ารันทุก A/B/C ด้วยผู้ใช้จริง

### S01 — หาลูกค้าถึงปิดการขาย · ได้บางส่วน · E01/E02/E11

- **เริ่ม/ข้อมูล:** Marketing รับ C001 ขอเฟอร์นิเจอร์ 20 ชุด งบ 100,000 กำหนดส่ง D+7; มี contact/consent, brief และเงื่อนไขบริการ
- **ส่งต่อ:** Marketing ส่ง lead → Sales รับ brief/opportunity → Legal รับ quote+ข้อสัญญา → Sales ออก approved quote/order → Operations รับรายการ/วันส่ง เจ้าของลูกค้าคือ Sales; หัวหน้า Sales ตามงานไม่รับภายใน 4 ชั่วโมง และแจ้งราคา/ข้อยกเว้นภายใน 1 วันทำการ
- **Cases:** A ชนะงานโดย brief ไม่หาย; B ลูกค้าเปลี่ยนจำนวน/เงื่อนไขหลังทบทวน ต้องใช้ revision ใหม่; C duplicate customer, consent revoked, lost opportunity ต้องคงประวัติและหยุดการใช้ข้อมูลที่ไม่อนุญาต
- **ผลลูกค้า:** ได้ข้อเสนอและคำยืนยันสิ่งที่จะส่งมอบตรงกันทุกฝ่าย
- **ตรวจพบ:** account/quote/order และ Legal แยกส่วนมีจริง; lead/opportunity/activity/forecast, consent/customer authority และ automatic obligation-to-order gate ยังไม่มีหลักฐานครบ งานเชื่อมต้องทำมือ → G01/G05

### S02 — ขายสินค้าจากสต็อกถึงเก็บเงิน · ได้บางส่วน · E02/E03/E04

- **เริ่ม/ข้อมูล:** Sales ขาย 10 ชิ้น × 1,000 ก่อนภาษีจากคลังมี 10 ชิ้น ต้นทุน 600/ชิ้น นัด D+2; fixture สมมติ tax 7% เพื่อทดสอบเลข ไม่ใช่คำแนะนำภาษี
- **ส่งต่อ:** Sales ส่ง approved order → คลังส่ง fulfilment 6+4 ชิ้น → Accounting รับ draft stock GL แล้ว invoice → ผู้ลงบัญชีอิสระ → Treasury รับยอดค้าง; Sales แจ้ง tracking/วันรับให้ C001 คลังรับงานภายใน 2 ชั่วโมง การเงินตามวันครบกำหนด
- **Cases:** A ส่ง 6+4, มูลค่ารวม 10,700 ต้นทุน 6,000 รับเงิน 4,280+6,420 ยอดค้างเป็นศูนย์; B เครดิตเกิน/งวดปิด/กดซ้ำต้องไม่เพิ่ม stock/GL/payment ซ้ำ; C ส่งช้า/ลูกค้าไม่รับของต้องมีผู้ติดตามและหลักฐานรับมอบ
- **ผลลูกค้า:** ของครบตามนัด ใบแจ้งหนี้และยอดรับเงินตรงกัน
- **ตรวจพบ:** กลไก partial delivery/settlement, named accounting/treasury, credit limit และ approved-order cancellation มีโค้ด/ชุดทดสอบ; shipment carrier/POD และการแจ้งลูกค้า end-to-end ยังไม่มีหลักฐานจบ → G02 ไม่ใช้ business-commands tests ทั่วไปแทนหลักฐานขายครบวงจร

### S03 — สินค้าไม่พอและต้องจัดซื้อ · ได้บางส่วน · E01/E02/E08/E10

- **เริ่ม/ข้อมูล:** C001 ต้องการ 10 ชิ้น มี 3 ชิ้น vendor ส่ง 4 ชิ้น D+2 และ 3 ชิ้น D+5
- **ส่งต่อ:** Sales ส่ง demand → Planning ตอบ shortage/วันส่ง → Purchasing ออก PO → Warehouse รับบางส่วน → Quality ตัดสินรับ/กัก → Accounting รับ GRNI/AP → Warehouse ส่งให้ C001 Sales ยืนยันวันใหม่ก่อนวันนัดเดิม 1 วัน; Purchasing ตาม vendor ทุกวันที่ค้าง
- **Cases:** A รับ/ส่งบางส่วนแล้วค้างถูกจำนวน; B vendor ส่งเสีย 2 ชิ้นต้องกันไม่ให้ขาย; C vendor เลื่อนนัดหรือราคา invoice ไม่ตรง PO ต้องมีเจ้าของ exception
- **ผลลูกค้า:** ทราบวันส่งที่ทำได้จริงและทางเลือกแบ่งส่ง
- **ตรวจพบ:** PO/partial receipt/AP และ internal purchase-request linkage มีจริง แต่ demand pegging/available-to-promise, RFQ, quality hold/release และ variance policy ขาด; internal request ไม่เท่ากับ procurement requisition ครบ → G03

### S04 — คืนสินค้าและคืนเงินหลังขาย · ได้บางส่วน · E02/E04/E05

- **เริ่ม/ข้อมูล:** ส่ง 10 ชิ้น ลูกค้าคืน 2 ชิ้น แยก invoice ที่ยังค้างกับ invoice ชำระครบ; คืนสินค้าเสียและ resalable แยกกัน
- **ส่งต่อ:** Sales/Support รับเหตุผล+delivery/invoice → Warehouse/Quality ตรวจสภาพ → ผู้อนุมัติอิสระ → Accounting ลดหนี้/กลับต้นทุน → Treasury คืนเงินและจับคู่หลักฐาน Sales แจ้งเลขรับเรื่องในวันเดียวและนัดคืนเงินภายใน 3 วันทำการหลังอนุมัติ
- **Cases:** A คืนไม่เกินจำนวนส่งและยอดค้าง; B จ่ายครบแล้วคืนบางส่วน ต้องได้เงินคืนจริง; C คืนซ้ำ/คืนเกิน/สินค้าชำรุดห้ามเพิ่มกลับเป็น stock พร้อมขายโดยไม่มี disposition
- **ผลลูกค้า:** ได้คืนเงินหรือสินค้าทดแทนตามข้อตกลง มีเลขอ้างอิงติดตาม
- **ตรวจพบ:** supplier return integration มี independent approval, stock reversal, debit-note GL; customer return ใช้ code branch ร่วมแต่ยังไม่มี test เฉพาะที่เทียบเท่าในไฟล์นี้ และ guard รับเฉพาะ posted/partially_paid พร้อมวงเงิน outstanding; paid return ไม่จบด้วย flow นี้ การ reversePayment คือแก้บันทึกผิด ไม่ใช่คืนเงินจริงให้ลูกค้า POS refund เป็นคนละ aggregate → G04

### S05 — ขายหน้าร้าน · ได้บางส่วน · E05/E15

- **เริ่ม/ข้อมูล:** SKU เหลือ 1 ชิ้น แคชเชียร์ 2 คน บิล 1,070 รับเงินสด 500+โอน 570 เครื่องสแกน/เครื่องพิมพ์พร้อม
- **ส่งต่อ:** Sales/แคชเชียร์ checkout → คลัง reservation/consume → Accounting รับ source postings → ผู้ตรวจปิด cash session; หัวหน้าร้านตาม payment exception ก่อนส่งของ ลูกค้าได้ receipt ทันทีเมื่อชำระยืนยัน
- **Cases:** A split tender และเงินทอน; B ซื้อชิ้นสุดท้ายพร้อมกัน/receipt callback ซ้ำ; C คืนบางส่วน+cash shortage และ provider คืนเงินยัง pending
- **ผลลูกค้า:** ไม่ขายเกิน stock ไม่คิดเงินซ้ำ มีใบเสร็จและคืนเงินติดตามได้
- **ตรวจพบ:** automated domain coverage มี reservation/refund/reconciliation/recovery แต่ Acme ยังไม่พร้อมขาย ไม่มี physical peripheral หรือ live payment acceptance รอบนี้ และพบ display currency risk → G06/G13

### S06 — ร้านอาหาร · ได้บางส่วน · E05

- **เริ่ม/ข้อมูล:** โต๊ะ T01 สั่งอาหาร 2 จาน ใช้วัตถุดิบตาม recipe/modifier พร้อมต้นทุนและครัว station
- **ส่งต่อ:** พนักงานส่ง kitchen ticket → ครัวรับ/เริ่ม/พร้อม → บริการยืนยันส่ง → Cashier ปิดบิล → Warehouse/Accounting รับ consume/WIP/COGS/waste หัวหน้าครัวตามรายการไม่รับ 5 นาที; บริการแจ้งลูกค้าเมื่อเกินเวลาที่ร้านกำหนด
- **Cases:** A แบ่ง/รวมบิลหลังส่งครัวไม่ทำอาหารซ้ำ; B ยกเลิกก่อนเริ่มปล่อย reservation หลังเริ่มลง waste; C คืนอาหารที่ทำแล้วไม่คืนวัตถุดิบเข้าคลัง
- **ผลลูกค้า:** ได้อาหารตามรายการ จ่ายบิลถูกและรู้เมื่อรายการล่าช้า
- **ตรวจพบ:** recipe, modifiers, WIP/waste, split/merge, refund มีใน tests; ยังไม่มี pilot ครัวจริง/หลักฐานแจ้งเวลารอลูกค้าและ staffing escalation → G06/G02

### S07 — โรงแรมครบการเข้าพัก · ได้บางส่วน · E06

- **เริ่ม/ข้อมูล:** C001 จอง 2 คืน ราคา 2,000/คืน มัดจำ 1,000 และ security deposit แยก; มีห้องหนึ่งห้อง housekeeping inspector และ night auditor คนละหน้าที่
- **ส่งต่อ:** Reservation ยืนยันราคา/policy → Front Office รับ arrival → Housekeeping clean → Supervisor inspect → Front Office check-in → Cashier folio/refund → Night Audit → Finance; maintenance block ต้อง inspected release ก่อนขาย หัวหน้า Front Office รับ escalation ห้องไม่พร้อมก่อนเวลา check-in ที่โรงแรมกำหนด
- **Cases:** A deposit/stay/night audit/checkout ไม่มี revenue ซ้ำ; B room move/extend/partial group cancel/no-show ตาม policy; C last-room race, room maintenance, audit หยุดกลางงาน, refund/revoked staff ต้อง recover โดยคงยอด
- **ผลลูกค้า:** จองแล้วมีห้องพร้อม บิลตรงนโยบาย ได้รับ confirmation และเงินค้ำคืนถูก
- **ตรวจพบ:** domain tests ครอบคลุมหลายเคส; ไม่มี hotel config ใน Acme, email จริง/online payment/OTA adapter ยังไม่ยืนยันหรือ deferred; no-show policy ต้อง UAT เพิ่ม ไม่อนุมานจาก cancellation test → G07

### S08 — ร้านอาหารลงบิลเข้าห้อง · ยังไม่มี flow เชื่อมจริง · E07

- **เริ่ม/ข้อมูล:** ผู้พัก in_house ห้อง 101, folio เปิด, POS บิล 535 บริษัท/สาขา/สกุลเงินเดียวกัน
- **ส่งต่อ:** Cashier ส่ง guest/stay+bill lines → Front Office ยืนยันสิทธิ์ลงห้อง → PMS รับ folio entry → Finance รับ revenue จาก owner เดียว; ถ้ารับไม่สำเร็จ cashier ต้องเห็น pending/failed ก่อนปิดบิล หัวหน้าแคชเชียร์ตามทันที
- **Cases:** A ลงห้องแล้ว checkout จ่ายรวมครั้งเดียว; B retry/timeout/checkout แข่งกับ charge ต้องไม่ซ้ำ; C void/refund แก้ทั้ง POS/PMS โดยไม่เปิด folio ปิดแล้วเอง
- **ผลลูกค้า:** ค่าอาหารอยู่ในบิลห้องที่ถูกต้อง ไม่ต้องจ่ายสองครั้ง
- **ตรวจพบ:** มี schema/prepareRoomCharge และ contract test เท่านั้น ไม่มี production caller และไม่มี room-charge tender ใน PaymentMethod → G08 ห้ามถือ minibar ของ PMS เป็น POS integration

### S09 — งานบริการและโครงการ · ได้บางส่วน · E08/E11/E02

- **เริ่ม/ข้อมูล:** สัญญาบริการ 100,000 แบ่ง milestone 40/60%, due D+10/D+20 มี project manager และผู้รับงานลูกค้า
- **ส่งต่อ:** Sales ส่ง scope/ราคา → Legal อนุมัติสัญญา → PM สร้าง tasks → Purchasing/People จัดทรัพยากร → PM ส่ง completion evidence → ลูกค้ารับมอบ → Accounting วางบิล → Treasury ตามเงิน PM แจ้งความคืบหน้ารายสัปดาห์และประเมิน change request ก่อนทำ
- **Cases:** A ส่งงานสองงวด; B ลูกค้า reject ต้อง rework และคง approval history; C เพิ่มขอบเขต/ต้นทุน/วันส่ง ต้อง approved revision ก่อน milestone invoice
- **ผลลูกค้า:** เห็นความคืบหน้า รับงานได้ และไม่ถูกคิดเงินงานที่ยังไม่รับ
- **ตรวจพบ:** projects/tasks/approval/rework และ service invoice มีแยกกัน; WBS dependencies/resource-cost, customer acceptance และ contract milestone → invoice อัตโนมัติยังไม่ครบ → G05

### S10 — บริการหลังขายและช่างหน้างาน · ยังไม่มี flow ครบ · E01/E08

- **เริ่ม/ข้อมูล:** C001 แจ้งอุปกรณ์เสีย อ้าง sales order/serial/warranty ที่ตั้งบริการและเวลาสะดวก
- **ส่งต่อ:** Support รับ ticket → Dispatcher นัดช่าง → Field technician ขออะไหล่ → Inventory/Procurement จ่าย → Technician ส่ง proof → ลูกค้ายืนยัน → Finance คิดเฉพาะนอกประกัน; service manager ตาม SLA รับเรื่อง 4 ชั่วโมง นัดหมายใน 1 วันทำการ
- **Cases:** A ซ่อมสำเร็จภายใต้ประกัน; B ไม่มีอะไหล่/ลูกค้าไม่อยู่/กลับมาซ่อมซ้ำ; C dispute ค่าใช้จ่ายต้องเปิดเรื่องต่อไม่ปิดตาม task completion ภายใน
- **ผลลูกค้า:** เลขเคสเดียวติดตามได้ตั้งแต่แจ้งถึงยืนยันแก้ไข
- **ตรวจพบ:** internal service request และ staff task ไม่ใช่ customer support ticket, warranty/dispatch/SLA/visit/billing release ยัง deferred → G01/G09

### S11 — ผลิตตามคำสั่งซื้อ · ยังไม่มี · E01

- **เริ่ม/ข้อมูล:** C001 สั่ง 50 หน่วย มี BOM version, วัตถุดิบและเวลาทำงานเครื่องจำกัด วันส่ง D+14
- **ส่งต่อ:** Sales demand → Planning capacity/material plan → Production work order → Warehouse issue → Quality release → Warehouse ship → Accounting cost/WIP; planner แจ้ง Sales ใน 1 วันเมื่อวันส่งเสี่ยงและ Sales แจ้ง C001
- **Cases:** A ผลิตครบตาม BOM; B ของเสีย/rework/วัตถุดิบทดแทนต้องอนุมัติ; C shortage/เครื่องเสียทำให้ split delivery พร้อม trace lot/cost
- **ผลลูกค้า:** ได้ของตามสเปก ทราบกำหนดส่ง และ trace/recall ได้
- **ตรวจพบ:** Manufacturing/Planning/Quality ไม่มี domain flow; recipe ของ POS ไม่ทดแทน production order/BOM genealogy → G10

### S12 — คำขอซื้อ เบิกค่าใช้จ่าย อุปกรณ์ และ IT · ได้บางส่วน · E03/E08/E09

- **เริ่ม/ข้อมูล:** พนักงานประจำ/contractor ที่มี engagement ขอ laptop 25,000 หรือเบิก 1,070 พร้อมหลักฐาน แผนกและสายบังคับบัญชาครบ
- **ส่งต่อ:** ผู้ขอ → หัวหน้า → Accounting reviewer → Purchasing/Service team → PO/รับของหรือผู้ลง GL อิสระ → Treasury → ผู้ขอยืนยันรับของ/เงิน หัวหน้าทีมรับใน 1 วันทำการ; requester เห็น My requests/Waiting และเหตุผลคืนแก้
- **Cases:** A partial reimbursement 400+670 ใช้หลักฐานจ่ายแยก; B reviewer ถูกถอนสิทธิ์/ผู้ขอเป็นหัวหน้า/route คลุมเครือต้อง reroute พร้อมเหตุผล; C service resolved ต้อง requester ยืนยัน ห้ามข้าม PO receipt แล้วบอกว่าซื้อสำเร็จ
- **ผลลูกค้าภายใน:** ได้อุปกรณ์/บริการ/เงินที่ร้องขอและเห็นสถานะ
- **ตรวจพบ:** workflow เป็นจุดแข็ง มี independent decisions, route freezing, retry, revoked actor, PO link และ handover; พบคิว Waiting จริงติด primary ไม่ eligible; asset custody/depreciation/warranty ยังไม่ครบ และ production procurement RFQ ไม่ใช่ส่วนเดียวกับ purchase request → G03/G11/G20

### S13 — กำลังคน การลา ย้าย และออกจากงาน · ได้บางส่วน · E09/E08

- **เริ่ม/ข้อมูล:** พนักงานกะข้ามคืน ลาครึ่งวัน มี shift swap และ task ลูกค้าค้าง; พนักงานอีกคนออก D+7 มี access cutoff ชัดเจน
- **ส่งต่อ:** HR engagement → หัวหน้าจัดกะ/อนุมัติ → HR review → Payroll hours → IT revoke → หัวหน้าโอนงาน → Finance/ทรัพย์สิน clearance; หัวหน้าแผนกรับผิดชอบหาคนแทนก่อนกะเริ่ม และแจ้งลูกค้าเฉพาะการเปลี่ยนผู้ประสานงานโดยไม่เผยข้อมูล HR
- **Cases:** A coworker ยอมรับ swap แล้วผู้จัดกะอิสระอนุมัติ; B leave หลัง publish/คนไม่ active/กะซ้อน; C cutoff มีผลแม้ worker ล่าช้า แต่ต้องส่งต่องาน/final pay แยกกัน
- **ผลลูกค้า:** ไม่มีงานถูกทิ้งเพราะคนลา/ย้าย/ออก และอดีตพนักงานไม่เข้าถึงข้อมูลต่อ
- **ตรวจพบ:** shifts, approval, cutoff และ clearance มี tests; automatic transfer งานทุก module และ payroll statutory/final-pay/bank payment ไม่ครบ การผ่าน clearance ไม่ได้จ่ายเงินเดือน → G11

### S14 — หลายสาขา กระทบยอด และปิดงวด · ได้บางส่วน · E10/E03/E04

- **เริ่ม/ข้อมูล:** คลังกรุงเทพฯโอน 10 ชิ้นต้นทุน 600 ไปภูเก็ต; เงินธนาคารมีค่าธรรมเนียม/รายการไม่ตรง; เพิ่ม Company B เพื่อทดสอบการกันข้อมูล
- **ส่งต่อ:** คลังต้นทาง → ผู้รับอิสระปลายทาง → Accounting variance/GL → Treasury bank matching → Controller close → ผู้บริหารรายงาน; Controller ตาม exception ก่อนปิดวันที่ D+3 และ Sales แจ้งลูกค้าเมื่อ stock in transit ส่งไม่ทัน
- **Cases:** A transfer quantity/value สมดุลและ bank match; B รับแค่ 7 อีก 3 อยู่ระหว่างทาง/สูญหาย; C backdated movement/closed period/cross-company read และเปิดงวดแก้ไขมี audit
- **ผลลูกค้า:** ของไม่หายระหว่างสาขา ยอดคงเหลือ/หนี้และรายงานตรงกัน
- **ตรวจพบ:** direct same-company transfer/count/GL มี implementation แต่ in-transit partial receiving, intercompany และ consolidation ไม่ครบ; opening subledger/full close acceptance ยังเป็น gap → G12

### S15 — ลูกค้าแพลตฟอร์ม ERPX · ได้บางส่วน · E12/E01/E14

- **เริ่ม/ข้อมูล:** Holding ใหม่ 2 บริษัทเลือก hosted support ระหว่างเดือน มีผู้ดูแลบัญชีและผู้รับผิดชอบบริการ
- **ส่งต่อ:** Onboarding → IAM/องค์กร → Platform operations → Billing reviewed actual cost → ผู้อนุมัติ → Stripe result → Support; account owner ยืนยันพร้อมใช้ก่อนส่ง login ให้ลูกค้า และดู payment failure ทุกวันทำการ
- **Cases:** A prorate/เปลี่ยนแผนปลายเดือน; B ค่าใช้จ่ายยังไม่ complete/ชำระล้มเหลว/webhook ซ้ำ; C cancel-hosted/withdraw/restore ต้องมีเงื่อนไขข้อมูลและเจ้าของ handoff
- **ผลลูกค้า:** เข้าใช้ได้ตามข้อตกลง เข้าใจบิลและทางแก้ปัญหา
- **ตรวจพบ:** billing sandbox มี flow; live billing/tax/refund/access restriction และ customer support/operational consoles ยังไม่ครบ ไม่ถือ Stripe sandbox verification เดิมเป็น live readiness รอบนี้ → G14

### Scenario เพิ่มเติมเพื่อไม่ให้โมดูลหลุด

| ID / ผล | ผู้เริ่ม ข้อมูล และจุดส่งต่อ | Cases / เวลา / ผลลูกค้า | หลักฐานและช่องว่าง |
|---|---|---|---|
| S16 E-commerce · ยังไม่มี | ลูกค้า C001 ใส่ cart 2 ชิ้น → ช่องทางออนไลน์ → Payment → Sales → Warehouse → Support | A checkout ส่งของ; B price/stock เปลี่ยนก่อนจ่าย; C payment success หลัง cart expire/return; Ecommerce manager ตาม payment exception ทันทีและแจ้ง order status | E01; channel/listing/cart/checkout/Sales handoff deferred; PMS booking portal ไม่ใช่ e-commerce ทั่วไป → G15 |
| S17 Talent · ยังไม่มี | หัวหน้าแผนกเปิดตำแหน่ง → Recruiting รับผู้สมัคร → ผู้สัมภาษณ์ → offer → HR onboarding → IT | A รับคนตาม approved requisition; B ถอนใบสมัคร/จำกัดข้อมูล; C ถอน offer ก่อน grant; HR ตามแต่ละรอบไม่เกิน 3 วันทำการและแจ้งผู้สมัคร | E01/E09; HR onboarding ไม่ครอบคลุม ATS/candidate/offer → G16 |
| S18 Benefits/Learning · ยังไม่มี flow เฉพาะ | HR เปิดแผน/หลักสูตร → พนักงานเลือกสิทธิ์/เรียน → provider/ผู้ประเมินรับรอง → Payroll/หัวหน้าอนุญาตทำงาน | A enrollment/completion; B เปลี่ยนผู้พึ่งพิง/ขาดเรียน; C provider reject/หมดอายุใบรับรอง; HR ตามก่อน enrollment cutoff/กะที่ต้องมี qualification | E01; handbook ไม่ใช่ LMS, roster ไม่ตรวจ professional qualification ครบ → G16 |
| S19 Brand/Platform governance · ได้บางส่วน | ผู้ดูแลเสนอ brand → ผู้ทบทวน → Sales/Legal ออกเอกสารในนาม Company; Ops backup → ผู้กู้คืน → เจ้าของระบบตรวจ | A issuer+brand ถูก; B draft rollback/ผิดสาขา; C restore แล้วกระทบยอด/สิทธิ์/ไฟล์; document owner ตรวจทุกครั้งก่อนส่ง ลูกค้าได้เอกสารนิติบุคคลถูก; Ops ตกลง RTO/RPO ก่อน pilot | E14/E01; brand approval/rollback และ recovery บน deployment จริงต้อง UAT; ไม่อนุมานจาก isolated recovery → G17 |
| S20 Collaboration/AI/Customer 360 · ได้บางส่วน | Sales/Support เปิด customer brief → mention/record relation → owner department → AI ร่างคำตอบโดยใช้ข้อมูลที่มีสิทธิ์ → คนยืนยันแล้วสื่อสารลูกค้า | A ข้อมูลอ้างอิงถูก; B source changed/permission revoked; C AI แนะนำอนุมัติหรือเปิดเผยข้อมูลลับต้องหยุด; record owner ตามใน SLA ต้นทาง ลูกค้าไม่ต้องเล่าเรื่องซ้ำ | E01/E13; unified service timeline/customer authority/action cards ยังขาด; agent catalog/session/pools ไม่ครบ → G01/G18 |
| S21 Legal evidence/hold/counsel · ได้บางส่วน | Legal รับข้อพิพาท C001 → เจ้าของเอกสาร Sales/HR/Finance ส่ง source → custodian รับ hold → counsel ส่ง advice/invoice → Legal ตรวจ → Purchasing/Finance รับคำขอซื้อ → ผู้อนุมัติ retention ตรวจ archive | A preserved document/share/signing/obligation evidence ตามสิทธิ์; B hold ซ้อน/withdraw share/revoke signer ต้องไม่รั่วหรือลบหลักฐาน; C external counsel ส่งบิลซ้ำ/retention worker หยุด ต้องไม่สร้างหนี้หรือทำลายซ้ำ; Legal owner ตาม deadline ที่คนกำหนดและให้ custodian รับทราบภายใน 1 วันทำการ | E11; มี tests เฉพาะ แต่ external onboarding/email/trust/retention policy และผู้ใช้จริงยังต้องรับรอง; ไม่รับรองผลบังคับตามกฎหมายจาก software test → G05/G17 |

## 4. Gap backlog ที่เชื่อมกับผลลูกค้า

P0 = ขวางบริการหลักของ scenario หรือเสี่ยงเงิน/สินค้า/ข้อมูลผิดเมื่อเปิดใช้; **ไม่ใช่คำกล่าวว่าพบ production incident** ทุกธุรกิจ P1 = ต้องทำมือ/เสี่ยงผิดนัด P2 = ประสิทธิภาพ/ความชัดเจน เกณฑ์รับด้านล่างคือข้อเสนอสำหรับงานแก้ถัดไป ไม่ใช่การอนุมัติให้แก้ในรอบ audit นี้

| ID / ระดับ | ขั้นที่ขาดและผลต่อลูกค้า | เจ้าของงานเสนอ | วิธีชั่วคราว | เกณฑ์รับงานแก้ไข |
|---|---|---|---|---|
| G01 P1 | Lead/opportunity → account → support timeline/authority ไม่ต่อ ลูกค้าต้องเล่าใหม่และไม่มี SLA กลาง | Sales + Customer Service + Product | บัญชีเคสภายนอกที่ควบคุมสิทธิ์ มี customer/order ID และผู้ตามรายวัน | lead เปลี่ยนเป็น quote โดย brief/consent อยู่ครบ; ticket เชื่อม order มี owner/due/escalation; revoke/merge ไม่รั่วข้อมูล |
| G02 P1 | ส่งมอบ → ลูกค้ารับ/แจ้งล่าช้าไม่ครบ | Logistics/Operations + Sales | แนบ POD และให้ Sales บันทึกเวลาที่แจ้งผ่านช่องทางที่ตกลง | partial dispatch/POD/ปฏิเสธรับเชื่อม order เดียว; overdue ขึ้นเจ้าของงานและแจ้งลูกค้าพร้อมหลักฐาน |
| G03 P1 | Shortage/RFQ/QC/variance และ supplier promise ไม่ต่อ | Planning + Purchasing + Quality | ตาราง shortage/วันส่ง แยกของรอตรวจและคนอนุมัติ variance | demand เชื่อม PO/receipts; rejected goods ขายไม่ได้; invoice mismatch มี approval ก่อน AP; นัดลูกค้าอัปเดตได้ |
| G04 P0 สำหรับร้านที่รับคืนหลังชำระ | Sales return จ่ายครบ/เกิน outstanding ไปคืนเงินไม่จบ; QC disposition ไม่ครบ | Sales + Warehouse + Finance/Treasury | กระบวนการคืนเงินภายนอกที่ Finance ควบคุมและกระทบยอด ห้ามทำ receipt reversal เทียมเพื่อหลบ guard | customer paid/partial return → independent refund → cash/provider evidence → bank match; original-cost stock/GL ถูก ไม่มีเกินยอด/ซ้ำ; damaged stock ไม่พร้อมขาย |
| G05 P1 | Contract/milestone/change → delivery acceptance → invoice ไม่ผูกเป็น flow เดียว | PMO + Legal + Sales + Finance | milestone register อ้าง immutable contract/version และใบรับงานก่อนวางบิล | scope revision ทำให้ผู้เกี่ยวข้องรับรองใหม่; reject/rework ไม่วางบิลอัตโนมัติ; customer acceptance มีสิทธิ์และหลักฐาน |
| G06 P0 ก่อนเปิด POS pilot | ยังไม่ตั้งบัญชี/เปิดขาย/กะ และไม่ยืนยัน provider+อุปกรณ์ | Store manager + Finance + IT | ใช้ระบบหน้าร้านเดิมระหว่าง acceptance | cash/split/คืนเงิน/ปิดกะครบผ่านเครื่องจริง; provider failure/retry/payout กระทบยอดได้; cashier ไม่ต้องมีสิทธิ์ manual GL |
| G07 P0 ก่อนเปิดจองโรงแรม | Acme ไม่มี property/rooms/rates/publication; OTA/online payment deferred | Hotel manager + Reservations + Finance | รับจองด้วยระบบโรงแรมที่ใช้อยู่ พร้อมเจ้าของ inventory เดียว | 2 บทบาทจองห้องสุดท้ายไม่ซ้ำ; ส่ง email จริง; housekeeping/inspection/checkout/refund/audit ผ่าน; adapter OTA แยก gate |
| G08 P0 สำหรับโรงแรมที่ต้องรวมค่าอาหาร | POS → PMS มีแค่ contract ไม่มี atomic tender | F&B + Front Office + Finance | เก็บค่าอาหารแยกพร้อม receipt ระหว่างยังไม่รองรับ room charge | bill/folio/payment/revenue source เชื่อมหนึ่งครั้ง; retry/void/refund/checkout race รวมยอดถูกและไม่รับรู้รายได้สองระบบ |
| G09 P0 สำหรับธุรกิจซ่อม/บริการภาคสนาม | ticket → warranty → dispatch → parts → customer signoff ไม่มี | Service manager + Operations | ใช้ระบบ ticket/dispatch ภายนอกที่มีเจ้าของและเชื่อม invoice | SLA pause/reopen/parts reservation/visit/no-show/ลูกค้าไม่ยอมรับ/chargeable invoice มี flow ครบ |
| G10 P0 สำหรับโรงงาน | ไม่มี production planning/BOM/work order/QC/WIP genealogy | Production + Planning + Cost accounting | ไม่ใช้ ERPX เป็นระบบควบคุมการผลิตหลัก | order → material/capacity → produce/scrap/rework → QC → ship ครบและต้นทุนกระทบ GL ได้ |
| G11 P0 สำหรับจ่ายเงินเดือน; P1 สำหรับส่งงาน | Payroll preview ไม่ใช่ statutory/final pay; asset custody และงานค้างทุก domain ไม่ครบ | HR + Payroll + IT + หัวหน้าแผนก | Payroll ภายนอก; checklist ส่งต่องาน/ทรัพย์สินตาม source โดยผู้รับยืนยัน | validated adapter/approved liabilities/payment+correction; cutoff ไม่รอ clearance; ทุก open customer work มีผู้รับใหม่และห้าม grant กลับเอง |
| G12 P1 | In-transit/partial transfer, intercompany, opening reconciliation/consolidated close ไม่ครบ | Warehouse controller + Financial controller | transfer โดยขั้นตอนที่รองรับและทะเบียน transit แยก; group consolidation ภายนอก | dispatch/partial receive/loss คง quantity/value; intercompany ไม่ใช้ same-company transfer; GL/subledger/opening/close/reopen กระทบยอดผ่าน |
| G13 P1 | POS แสดง THB แบบคงที่เมื่อ Commerce ใช้ USD อาจทำให้เข้าใจสกุลเงินผิด | POS product + Finance | หยุด pilot non-THB จน UI/receipt ตรวจตรง configuration | currency จาก record/company ถึง catalog/cart/receipt/refund; test THB และ USD แยกจากกติกา Stripe THB; ไม่เปลี่ยนยอดเพื่อแก้ label |
| G14 P0 ก่อนขาย hosted แบบ live | Sandbox billing ยังไม่พิสูจน์ live/tax/refund/access restriction และ support | Platform commercial + Finance + Support | เปิดเฉพาะ sandbox/ขอบเขตบริการที่ทีมดูแลเองตามข้อตกลง | billing failure/recovery/cancel/export/support escalation ผ่าน live-approved pilot โดยไม่ตัดสิทธิ์ผิด Holding |
| G15 P0 สำหรับเว็บขายสินค้า | ไม่มี commerce storefront → cart → payment → fulfillment | Ecommerce + Sales + Warehouse | ระบบร้านค้าเดิม | paid order/webhook/retry/stock hold/return เชื่อม Sales ตาม source ID เดียว |
| G16 P1 | Recruiting/benefits/LMS ยังไม่มี workflow | HR + ผู้ให้บริการสวัสดิการ/ฝึกอบรม | ระบบ HR เฉพาะทางพร้อม approved import/handoff | candidate→hire ไม่แชร์ข้อมูลเกินสิทธิ์; benefits provider acknowledgement→payroll; training expiry→eligibility มี owner |
| G17 P1 | brand lifecycle/operational recovery และ readiness evidence ยังไม่ครบ | Admin + Ops + Document owner | ผู้ตรวจเอกสารและ runbook แบบมีผู้รับรอง | draft/category lock/rollback ไม่เปลี่ยน issuer; restore จริงผ่าน RTO/RPO ที่ลูกค้ารับรอง และเงิน/ไฟล์/สิทธิ์ตรง |
| G18 P1; ส่วนจัดระเบียบ P2 | AI/record relations/action cards ไม่ครอบคลุมทุก domain | Product + Data owners | คนเปิด source ตรวจและส่งต่อด้วย named task | source ACL/version drift ปิดช่องรั่ว; generated draft ไม่ตัดสินใจแทนคน; relation/backlink/dead link และ retention ผ่าน |
| G19 P1 | Test runner/runbook/environment ไม่สอดคล้อง ทำให้รายงาน pass ทำซ้ำยาก | Engineering/QA | โหลดเฉพาะ test URL ที่ตั้งไว้และใช้ guard เดิม | คำสั่ง documented รันบน pinned toolchain ได้โดยไม่แสดง secret; ระบุ port ตรง runtime; test skip/failure ไม่ถูกนับว่าผ่าน |
| G20 P1 | Work → Waiting มี expense ติด primary ไม่ eligible; งานไม่เดินต่อจนแก้ผู้รับ | Company admin + เจ้าของคำขอ + Finance | เปิด Flow ตรวจคุณสมบัติแล้ว reassign พร้อมเหตุผลโดยผู้มีอำนาจ; audit นี้ไม่เปลี่ยน assignment | ผู้รับใหม่ eligible/independent เห็น next action, ผู้รับเดิมทำต่อไม่ได้, history เดิมอยู่; วัดเวลาที่งานติดและมีเจ้าของตาม |

**ข้อแตกต่างที่ไม่ควรสรุปผิด:** reservations ของ POS มีอยู่แล้ว แต่ยังไม่ใช่ shared demand reservation ของ Sales/Planning; PMS maintenance มี room block/inspect แต่ไม่ใช่ asset maintenance เต็มรูปแบบ; Legal SLA มีเฉพาะ Legal ไม่ได้ทำให้ customer Support SLA พร้อม; internal purchase request ใช้งานได้แต่ RFQ ยังขาด; payment correction ไม่ใช่ refund; generic references ไม่ได้สร้าง downstream obligation อัตโนมัติ

## 5. แผนทดลองใช้และ UAT ที่ต้องให้คนทำงานรับรอง

| ประเภทธุรกิจ | ทดลองได้ในขอบเขต | เงื่อนไขก่อนใช้กับลูกค้าจริง |
|---|---|---|
| ซื้อขายสินค้า B2B | quote/order/PO/stock/GL/settlement ที่ระบุเจ้าของชัดเจน | G02–G04; ทดสอบ customer return และแจ้งส่งมอบจริง; ตรวจ opening/close |
| ร้านค้าปลีก/ร้านอาหาร | domain simulation และเตรียม settings/recipes | G06/G13, เครื่องจริง, cash/refund/provider reconciliation; ไม่รับ room charge จน G08 ผ่าน |
| โรงแรม | simulation จอง/เข้าพัก/แม่บ้าน/คืนเงิน/night audit | G07; ทดสอบ customer email/policy, ตั้ง hotel/publish; OTA/online payments เป็น gate เพิ่ม |
| โรงแรมร่วม F&B | ใช้สองส่วนแยกตาม acceptance ของแต่ละส่วน | ห้ามสรุปว่าปิดบิลรวมได้ก่อน G08 |
| บริษัทบริการ/โครงการ | internal tasks/review/evidence และ Legal ภายใน | G05/G09; customer acceptance/milestone invoicing และ service SLA |
| โรงงาน | งานสนับสนุนและการซื้อขายที่แยกจากการผลิต | G10; ยังไม่เหมาะใช้เป็น production-control system |
| HR/IT/งานภายใน | requests/leave/roster/placement/access lifecycle | G11/G16; mapping พนักงาน/approver พร้อม; ใช้ payroll ภายนอกสำหรับการจ่ายจริง |
| ผู้ให้บริการ ERPX | sandbox enrollment/billing และ onboarding ที่ตรวจแล้ว | G14/G17; support และ recovery ใน environment จริง |

### แบบรับรอง UAT ต่อ scenario

เจ้าของธุรกิจกรอก: scenario/case ID, tenant/company/branch, วันที่/เวอร์ชันระบบ, ผู้ขอ, ผู้ส่งและผู้รับแต่ละแผนก, source document IDs, เวลาส่ง/รับ/ครบกำหนด, ผู้ตาม escalation, สิ่งที่ลูกค้าเห็น/ได้รับ, stock/GL/payment ก่อน–หลัง, evidence link, ผล expected/actual, defect ID, ผู้รับรองและวันรับรอง

1. ทำ A ด้วยคนต่างบทบาทจริงและหนึ่งตัวแทนลูกค้า ไม่ใช้ owner เดียวรับรองสิทธิ์ทุกแผนก
2. ทำ B/C ที่เกี่ยวข้อง รวมแก้ไข/คืนงาน/ยกเลิก/บางส่วน/ถอนสิทธิ์/กดซ้ำ/พร้อมกัน และ service failure ตรวจว่างานไม่สูญหายและยอดไม่ซ้ำ
3. ให้ผู้รับงานเปิด My Work/หน้าคิวของตนโดยไม่ส่ง deep link ให้ก่อน ตรวจว่าเห็นงานและรู้ next action; ทดสอบสิทธิ์ Department เทียบ Branch โดยเฉพาะ POS/PMS ซึ่ง contract ไม่รองรับ department scope
4. ให้ลูกค้าตรวจข้อความยืนยัน นัดหมาย ใบเสร็จ/บิล หรือผลแก้ไขจริง และลองติดตามเคสโดยไม่ถามหลายแผนก การที่พนักงานภายในกด completed อย่างเดียวไม่พอ
5. QA/เจ้าของงาน sign off เฉพาะ scenario ที่ไม่มี P0 ค้างและ P1 มีผู้รับผิดชอบ/วิธีชั่วคราวที่ธุรกิจยอมรับ; provider ที่ skipped, fake email, simulated bank ไม่ถือเป็น live verified

## 6. Traceability ครบทุก capability

ตารางต่อไปเป็น **การจัดเข้าชุดตรวจ ไม่ใช่ผลทดสอบราย capability** นับจากทุก section ที่ `capability=true` ไม่เลือกเฉพาะเมนูที่เปิดได้ และไม่ใช้ exclusions ของ generator ที่ตัด Legal ออก สถานะ registry เก็บเพื่อเทียบกับ verdict ใน scenario; การมี test file ไม่พิสูจน์ทุกแถวที่อ้างไฟล์เดียวกัน

<!-- CAPABILITY-TRACEABILITY -->

ครบ **183/183 capabilities, 32/32 modules; ไม่มีรายการตกหล่นหรือถูกยกเว้น** Legal ใช้ E11 แทนสถานะ generator ส่วนความสามารถนอก registry เช่น roster, calendar, executive และ Legal specialized registers อยู่ใน S13/S14/S20/S21 ด้วย

| Module / capability ID | Capability | Registry status | Scenario | หลักฐาน | Gap ที่ต้องอ่านร่วม |
|---|---|---|---|---|---|
| `brand/feature-draft-workspace` | Draft workspace | implemented | S19 | E14 | G17 |
| `brand/feature-category-locks` | Category locks | implemented | S19 | E14 | G17 |
| `brand/feature-published-identity` | Published identity | implemented | S19 | E14 | G17 |
| `commercial/invoices` | Invoices | sandbox_verified | S15 | E12 | G14 |
| `commercial/entitlements` | Entitlements | sandbox_verified | S15 | E12 | G14 |
| `commercial/feature-catalog-plans` | Catalog plans | sandbox_verified | S15 | E12 | G14 |
| `commercial/feature-subscriptions` | Subscriptions | sandbox_verified | S15 | E12 | G14 |
| `commercial/feature-grace-recovery` | Grace & recovery | sandbox_verified | S15 | E12 | G14 |
| `identity_access/assignments` | Assignments | flow_accepted | S12/S13/S15 | E09 | G11/G14 |
| `identity_access/access-reviews` | Access reviews | implemented | S12/S13/S15 | E09 | G11/G14 |
| `identity_access/feature-people-principals` | People & principals | flow_accepted | S12/S13/S15 | E09 | G11/G14 |
| `identity_access/feature-roles-permissions` | Roles & permissions | flow_accepted | S12/S13/S15 | E09 | G11/G14 |
| `identity_access/feature-directory-connectors` | Directory connectors | live_verified | S12/S13/S15 | E09 | G11/G14 |
| `identity_access/feature-identity-conflicts` | Identity conflicts | implemented | S12/S13/S15 | E09 | G11/G14 |
| `organization/organization-tree` | Organization tree | implemented | S12/S13 | E09 | G11 |
| `organization/sharing` | Sharing | implemented | S12/S13 | E09 | G11 |
| `organization/feature-node-lifecycle` | Node lifecycle | implemented | S12/S13 | E09 | G11 |
| `organization/feature-business-units` | Business units | implemented | S12/S13 | E09 | G11 |
| `platform_administration/provisioning` | Provisioning | implemented | S15/S19 | E14 | G14/G17 |
| `platform_administration/feature-holdings` | Holdings | implemented | S15/S19 | E14 | G14/G17 |
| `platform_administration/feature-connectors` | Connectors | implemented | S15/S19 | E14 | G14/G17 |
| `platform_administration/feature-infrastructure` | Infrastructure | implemented | S15/S19 | E14 | G14/G17 |
| `platform_administration/feature-backup-recovery` | Backup & recovery | implemented | S15/S19 | E14 | G14/G17 |
| `platform_administration/feature-projections-events` | Projections & events | implemented | S15/S19 | E14 | G14/G17 |
| `platform_administration/feature-migration-runs` | Migration runs | implemented | S15/S19 | E14 | G14/G17 |
| `platform_administration/feature-temporary-access` | Temporary access | implemented | S15/S19 | E14 | G14/G17 |
| `platform_administration/feature-platform-audit` | Platform audit | implemented | S15/S19 | E14 | G14/G17 |
| `customer_360/sales-summary` | Sales summary | implemented | S01/S20 | E01/E02 | G01 |
| `customer_360/service-timeline` | Service timeline | deferred | S01/S20 | E01/E02 | G01 |
| `customer_360/feature-customer-profile` | Customer profile | implemented | S01/S20 | E01/E02 | G01 |
| `customer_360/feature-customer-notes` | Customer notes | deferred | S01/S20 | E01/E02 | G01 |
| `customer_authority/duplicate-review` | Duplicate review | deferred | S01/S20 | E01 | G01 |
| `customer_authority/access-history` | Access history | deferred | S01/S20 | E01 | G01 |
| `customer_authority/feature-authority-bindings` | Authority bindings | deferred | S01/S20 | E01 | G01 |
| `customer_authority/feature-identity-changes` | Identity changes | deferred | S01/S20 | E01 | G01 |
| `support/tickets` | Tickets | deferred | S04/S10/S15 | E01 | G01/G09 |
| `support/sla-watch` | SLA watch | deferred | S04/S10/S15 | E01 | G01/G09 |
| `support/incidents` | Incidents | deferred | S04/S10/S15 | E01 | G01/G09 |
| `support/feature-support-sessions` | Support sessions | deferred | S04/S10/S15 | E01 | G01/G09 |
| `support/feature-capabilities` | Capabilities | deferred | S04/S10/S15 | E01 | G01/G09 |
| `support/feature-attachments` | Attachments | deferred | S04/S10/S15 | E01 | G01/G09 |
| `finance/journals` | Journals | flow_accepted | S02/S04/S14 | E02/E04/E10 | G04/G12 |
| `finance/payables` | Payables | flow_accepted | S02/S04/S14 | E02/E04/E10 | G04/G12 |
| `finance/feature-chart-of-accounts` | Chart of accounts | implemented | S02/S04/S14 | E02/E04/E10 | G04/G12 |
| `finance/feature-payments` | Payments | flow_accepted | S02/S04/S14 | E02/E04/E10 | G04/G12 |
| `finance/feature-receivables-aging` | Receivables & aging | implemented | S02/S04/S14 | E02/E04/E10 | G04/G12 |
| `finance/feature-bank-reconciliation` | Bank reconciliation | flow_accepted | S02/S04/S14 | E02/E04/E10 | G04/G12 |
| `finance/feature-reports` | Reports | implemented | S02/S04/S14 | E02/E04/E10 | G04/G12 |
| `finance/feature-period-close` | Period close | implemented | S02/S04/S14 | E02/E04/E10 | G04/G12 |
| `finance/feature-opening-balances` | Opening balances | implemented | S02/S04/S14 | E02/E04/E10 | G04/G12 |
| `pos/refunds` | Refunds | flow_accepted | S05/S06/S08 | E05/E07 | G06/G08/G13 |
| `pos/feature-register-session` | Register session | flow_accepted | S05/S06/S08 | E05/E07 | G06/G08/G13 |
| `pos/feature-sales-baskets` | Sales & baskets | flow_accepted | S05/S06/S08 | E05/E07 | G06/G08/G13 |
| `pos/feature-payments-receipts` | Payments & receipts | flow_accepted | S05/S06/S08 | E05/E07 | G06/G08/G13 |
| `pos/feature-voids` | Voids | flow_accepted | S05/S06/S08 | E05/E07 | G06/G08/G13 |
| `sales/quotes` | Quotes | flow_accepted | S01/S02/S04 | E02/E04 | G02/G04 |
| `sales/orders` | Orders | flow_accepted | S01/S02/S04 | E02/E04 | G02/G04 |
| `sales/fulfillment` | Fulfillment | flow_accepted | S01/S02/S04 | E02/E04 | G02/G04 |
| `sales/feature-cancellations` | Cancellations | flow_accepted | S01/S02/S04 | E02/E04 | G02/G04 |
| `sales/feature-revenue-handoff` | Revenue handoff | flow_accepted | S01/S02/S04 | E02/E04 | G02/G04 |
| `assets/asset-register` | Asset register | deferred | S12/S13 | E01 | G11 |
| `assets/condition` | Condition | deferred | S12/S13 | E01 | G11 |
| `assets/feature-placement` | Placement | deferred | S12/S13 | E01 | G11 |
| `assets/feature-maintenance-lifecycle` | Maintenance lifecycle | deferred | S12/S13 | E01 | G11 |
| `assets/feature-retirement` | Retirement | deferred | S12/S13 | E01 | G11 |
| `inventory/availability` | Availability | flow_accepted | S02/S03/S14 | E02/E10 | G03/G12 |
| `inventory/reservations` | Reservations | deferred | S02/S03/S14 | E02/E10 | G03/G12 |
| `inventory/feature-stock-register` | Stock register | flow_accepted | S02/S03/S14 | E02/E10 | G03/G12 |
| `inventory/feature-receipts` | Receipts | flow_accepted | S02/S03/S14 | E02/E10 | G03/G12 |
| `inventory/feature-issues-movements` | Issues & movements | flow_accepted | S02/S03/S14 | E02/E10 | G03/G12 |
| `maintenance/work-orders` | Work orders | deferred | S07/S10 | E01/E06 | G09 |
| `maintenance/downtime` | Downtime | deferred | S07/S10 | E01/E06 | G09 |
| `maintenance/feature-blocked-work` | Blocked work | deferred | S07/S10 | E01/E06 | G09 |
| `maintenance/feature-completion` | Completion | deferred | S07/S10 | E01/E06 | G09 |
| `maintenance/feature-maintenance-history` | Maintenance history | deferred | S07/S10 | E01/E06 | G09 |
| `operations/feature-exception-queue` | Exception queue | implemented | S09/S10/S12 | E08 | G02/G05/G09 |
| `operations/feature-escalations` | Escalations | implemented | S09/S10/S12 | E08 | G02/G05/G09 |
| `operations/feature-approval-decisions` | Approval decisions | implemented | S09/S10/S12 | E08 | G02/G05/G09 |
| `operations/feature-resolution` | Resolution | implemented | S09/S10/S12 | E08 | G02/G05/G09 |
| `operations/feature-queue-policies` | Queue policies | implemented | S09/S10/S12 | E08 | G02/G05/G09 |
| `operations/feature-operations-summary` | Operations summary | implemented | S09/S10/S12 | E08 | G02/G05/G09 |
| `procurement/requisitions` | Requisitions | deferred | S03/S12 | E02/E08 | G03 |
| `procurement/purchase-orders` | Purchase orders | flow_accepted | S03/S12 | E02/E08 | G03 |
| `procurement/goods-receipts` | Goods receipts | flow_accepted | S03/S12 | E02/E08 | G03 |
| `procurement/feature-invoice-match` | Invoice match | implemented | S03/S12 | E02/E08 | G03 |
| `procurement/feature-supplier-history` | Supplier history | implemented | S03/S12 | E02/E08 | G03 |
| `quality/inspections` | Inspections | deferred | S03/S04/S11 | E01 | G03/G04/G10 |
| `quality/results` | Results | deferred | S03/S04/S11 | E01 | G03/G04/G10 |
| `quality/release-gates` | Release gates | deferred | S03/S04/S11 | E01 | G03/G04/G10 |
| `quality/feature-cancellation` | Cancellation | deferred | S03/S04/S11 | E01 | G03/G04/G10 |
| `legal/contracts` | Contracts | Legal: ประเมินแยก | S01/S09/S21 | E11 | G05/G17 |
| `legal/feature-documents` | Documents | Legal: ประเมินแยก | S01/S09/S21 | E11 | G05/G17 |
| `legal/feature-legal-holds` | Legal holds | Legal: ประเมินแยก | S01/S09/S21 | E11 | G05/G17 |
| `legal/feature-document-access` | Document access | Legal: ประเมินแยก | S01/S09/S21 | E11 | G05/G17 |
| `legal/feature-archive` | Archive | Legal: ประเมินแยก | S01/S09/S21 | E11 | G05/G17 |
| `people_payroll/employees` | Employees | flow_accepted | S12/S13 | E09 | G11 |
| `people_payroll/payroll-summaries` | Payroll summaries | implemented | S12/S13 | E09 | G11 |
| `people_payroll/feature-leave-requests` | Leave requests | flow_accepted | S12/S13 | E09 | G11 |
| `people_payroll/feature-expenses` | Expenses | flow_accepted | S12/S13 | E09 | G11 |
| `people_payroll/feature-payroll-runs` | Payroll runs | implemented | S12/S13 | E09 | G11 |
| `crm/accounts` | Accounts | implemented | S01/S20 | E01/E02 | G01 |
| `crm/activities` | Activities | deferred | S01/S20 | E01/E02 | G01 |
| `crm/feature-leads` | Leads | deferred | S01/S20 | E01/E02 | G01 |
| `crm/feature-opportunities` | Opportunities | deferred | S01/S20 | E01/E02 | G01 |
| `crm/feature-sales-handoff` | Sales handoff | implemented | S01/S20 | E01/E02 | G01 |
| `marketing/feature-forms-profiles` | Forms & profiles | deferred | S01/S16 | E01 | G01/G15 |
| `marketing/feature-content-previews` | Content & previews | deferred | S01/S16 | E01 | G01/G15 |
| `marketing/feature-domains` | Domains | deferred | S01/S16 | E01 | G01/G15 |
| `marketing/feature-provider-readiness` | Provider readiness | deferred | S01/S16 | E01 | G01/G15 |
| `marketing/feature-lead-candidates` | Lead candidates | deferred | S01/S16 | E01 | G01/G15 |
| `marketing/feature-engagement` | Engagement | deferred | S01/S16 | E01 | G01/G15 |
| `manufacturing/orders` | Orders | deferred | S11 | E01 | G10 |
| `manufacturing/material-readiness` | Material readiness | deferred | S11 | E01 | G10 |
| `manufacturing/quality-disposition` | Quality disposition | deferred | S11 | E01 | G10 |
| `manufacturing/feature-specifications` | Specifications | deferred | S11 | E01 | G10 |
| `manufacturing/feature-labor-output` | Labor & output | deferred | S11 | E01 | G10 |
| `manufacturing/feature-costs-genealogy` | Costs & genealogy | deferred | S11 | E01 | G10 |
| `planning/scenarios` | Scenarios | deferred | S03/S11 | E01 | G03/G10 |
| `planning/shortages` | Shortages | deferred | S03/S11 | E01 | G03/G10 |
| `planning/recommendations` | Recommendations | deferred | S03/S11 | E01 | G03/G10 |
| `planning/feature-capacity` | Capacity | deferred | S03/S11 | E01 | G03/G10 |
| `planning/feature-plan-comparison` | Plan comparison | deferred | S03/S11 | E01 | G03/G10 |
| `projects/dependencies` | Dependencies | implemented | S09 | E08 | G05 |
| `projects/feature-projects-wbs` | Projects & WBS | implemented | S09 | E08 | G05 |
| `projects/feature-handoffs` | Handoffs | implemented | S09 | E08 | G05 |
| `projects/feature-time-costs` | Time & costs | implemented | S09 | E08 | G05 |
| `projects/feature-attachments` | Attachments | implemented | S09 | E08 | G05 |
| `projects/feature-project-graph` | Project graph | implemented | S09 | E08 | G05 |
| `ecommerce/channels` | Channels | deferred | S16 | E01 | G15 |
| `ecommerce/listings` | Listings | deferred | S16 | E01 | G15 |
| `ecommerce/returns` | Returns | deferred | S16 | E01 | G15 |
| `ecommerce/feature-carts` | Carts | deferred | S16 | E01 | G15 |
| `ecommerce/feature-checkout` | Checkout | deferred | S16 | E01 | G15 |
| `ecommerce/feature-fulfillment` | Fulfillment | deferred | S16 | E01 | G15 |
| `benefits/dependents` | Dependents | deferred | S18 | E01 | G16 |
| `benefits/coverage` | Coverage | deferred | S18 | E01 | G16 |
| `benefits/feature-plans` | Plans | deferred | S18 | E01 | G16 |
| `benefits/feature-enrollment-windows` | Enrollment windows | deferred | S18 | E01 | G16 |
| `benefits/feature-provider-payroll` | Provider & payroll | deferred | S18 | E01 | G16 |
| `learning/course-catalog` | Course catalog | deferred | S18 | E01 | G16 |
| `learning/feature-assignments` | Assignments | deferred | S18 | E01 | G16 |
| `learning/feature-attendance-results` | Attendance & results | deferred | S18 | E01 | G16 |
| `learning/feature-completion` | Completion | deferred | S18 | E01 | G16 |
| `learning/feature-eligibility-handoff` | Eligibility handoff | deferred | S18 | E01 | G16 |
| `talent/requisitions` | Requisitions | deferred | S17 | E01 | G16 |
| `talent/applications` | Applications | deferred | S17 | E01 | G16 |
| `talent/offers` | Offers | deferred | S17 | E01 | G16 |
| `talent/feature-candidates-privacy` | Candidates & privacy | deferred | S17 | E01 | G16 |
| `talent/feature-hire-handoffs` | Hire handoffs | deferred | S17 | E01 | G16 |
| `talent/feature-proposals` | Proposals | deferred | S17 | E01 | G16 |
| `field_service/service-orders` | Service orders | deferred | S10 | E01 | G09 |
| `field_service/feature-agreements` | Agreements | deferred | S10 | E01 | G09 |
| `field_service/feature-dispatch` | Dispatch | deferred | S10 | E01 | G09 |
| `field_service/feature-visits` | Visits | deferred | S10 | E01 | G09 |
| `field_service/feature-exceptions` | Exceptions | deferred | S10 | E01 | G09 |
| `field_service/feature-billing-release` | Billing release | deferred | S10 | E01 | G09 |
| `agent/agent-catalog` | Agent catalog | deferred | S20 | E13 | G18 |
| `agent/versions` | Versions | deferred | S20 | E13 | G18 |
| `agent/assignments` | Assignments | deferred | S20 | E13 | G18 |
| `agent/feature-pools-routing` | Pools & routing | deferred | S20 | E13 | G18 |
| `agent/feature-private-sessions` | Private sessions | deferred | S20 | E13 | G18 |
| `agent/feature-field-suggestions` | Field suggestions | implemented | S20 | E13 | G18 |
| `agent/feature-invocation-evidence` | Invocation evidence | deferred | S20 | E13 | G18 |
| `collaboration/threads` | Threads | implemented | S12/S20 | E08/E13 | G18 |
| `collaboration/mentions` | Mentions | implemented | S12/S20 | E08/E13 | G18 |
| `collaboration/feature-comments-moderation` | Comments & moderation | flow_accepted | S12/S20 | E08/E13 | G18 |
| `collaboration/feature-following-reactions` | Following & reactions | flow_accepted | S12/S20 | E08/E13 | G18 |
| `collaboration/feature-topics-feed` | Topics & feed | flow_accepted | S12/S20 | E08/E13 | G18 |
| `collaboration/feature-attachments` | Attachments | deferred | S12/S20 | E08/E13 | G18 |
| `collaboration/feature-action-cards` | Action cards | deferred | S12/S20 | E08/E13 | G18 |
| `collaboration/feature-record-relations` | Record relations | deferred | S12/S20 | E08/E13 | G18 |
| `pms/dashboard` | Overview | flow_accepted | S07/S08 | E06/E07 | G07/G08 |
| `pms/reservations` | Reservations | flow_accepted | S07/S08 | E06/E07 | G07/G08 |
| `pms/front-desk` | Front Office | flow_accepted | S07/S08 | E06/E07 | G07/G08 |
| `pms/rooms` | Rooms | flow_accepted | S07/S08 | E06/E07 | G07/G08 |
| `pms/housekeeping` | Housekeeping | flow_accepted | S07/S08 | E06/E07 | G07/G08 |
| `pms/folios` | Folios & payments | flow_accepted | S07/S08 | E06/E07 | G07/G08 |
| `pms/cashier` | Cashier shifts | flow_accepted | S07/S08 | E06/E07 | G07/G08 |
| `pms/inventory` | Minibar & linen | flow_accepted | S07/S08 | E06/E07 | G07/G08 |
| `pms/guests` | Guests | flow_accepted | S07/S08 | E06/E07 | G07/G08 |
| `pms/reports` | Reports | flow_accepted | S07/S08 | E06/E07 | G07/G08 |
| `pms/night-audit` | Night Audit | flow_accepted | S07/S08 | E06/E07 | G07/G08 |
| `pms/settings` | Settings | flow_accepted | S07/S08 | E06/E07 | G07/G08 |

### ข้อจำกัดของทะเบียนและหลักฐานที่ตรวจพบ

- หลาย capability ใช้ service/test เดียวกันและสืบทอดสถานะทั้งโมดูล จึงยังไม่มี assertion ราย capability โดยอัตโนมัติ; appendix นี้พิสูจน์ coverage ของรายการ ไม่ได้เพิ่มสถานะผ่านให้รายการใด
- E01 ระบุเส้นทาง preview แบบ template บางแห่งที่ไม่มี route ตรงชื่อใน tree ปัจจุบัน ใช้ liveDestination และ route/service จริงในการตรวจ ห้ามนำ path placeholder ไปนับเป็นหน้าที่ใช้งานได้
- Legal ไม่อยู่ใน capability-delivery generator แต่มี service/schema/tests ใหม่ และ 5 catalog entries ไม่สะท้อน specialized workflows ทั้งหมด จึงเพิ่ม S21 โดยเฉพาะ
- การโอนคลังมี HTTP verification script เดิม แต่ไม่ได้รัน script ที่เขียนลง development database ใน audit นี้; S14 จึงยังไม่มี fresh browser/HTTP transfer acceptance
- ชุดตัวเลขทดสอบในเอกสารเก่าอาจมาก/น้อยกว่ารอบนี้เพราะ workspace เปลี่ยน ให้ยึดผลรันและเวลาที่บันทึกไว้ในหัวข้อ 2 ไม่สรุปจากจำนวนว่าการตรวจใหม่ครอบคลุมมากกว่าเดิม

<!-- /CAPABILITY-TRACEABILITY -->
