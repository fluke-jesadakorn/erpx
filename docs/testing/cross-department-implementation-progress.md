# ERPX — ความคืบหน้าการพัฒนา flow ข้ามแผนก

วันที่ 2026-09-20 · สถานะ: กำลังพัฒนา ยังไม่ครบแผนช่วง 0–8

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
- ยังเหลือ partial/split requisition allocations, UOM conversion, supplier invitations/portal, price/quantity invoice variance, lot/serial/landed cost และชุด A–J ส่วนอื่น. ไม่ประกาศปิด C หรือทั้งแผน.

คู่มือและขอบเขต [RFQ simulation](procurement-rfq-simulation.md). Migrations0209–0210ใช้กับdevแบบadditive; production3101/Tunnelไม่เปลี่ยน.


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

Customer access/authority เพิ่มแล้ว เหลือ **68 Deferred**: [คู่มือ](customer-access.md), [23 tests](evidence/2026-09-20-customer-access-tests.txt), [context](evidence/2026-09-20-customer-access-context.json), [browser 3 บทบาท](evidence/2026-09-20-customer-access-browser.json) และ [source records](evidence/2026-09-20-customer-access-browser-records.json). Binding ผูก approved source+verified email+expiry, ต้อง independent review, first verified account ถูก bind ชัดเจน, ทุก portal view มี audit และ revoke แล้ว projection หายทันที. Portal ใช้ explicit field allowlist; internal notes/cost/HR/Legal/private Support evidence ไม่ถูกส่ง. ยังเหลือ consent, delegated customer admin, reply/upload/email และ UAT จริง.

Customer duplicate review เพิ่มแล้ว เหลือ **67 Deferred**: [คู่มือ](customer-duplicate-reviews.md), [11 tests](evidence/2026-09-20-customer-duplicates-tests.txt) และ [context](evidence/2026-09-20-customer-duplicates-context.json). คู่ customer ถูก canonicalize กันยื่นกลับด้าน, independent current manager ตัดสิน distinct/duplicate/reopen และเก็บ canonical choice/history; ยังไม่ย้าย source records หรือกระทบยอด จึงไม่อ้างว่า merge transaction เสร็จ.

Customer identity change เพิ่มแล้ว เหลือ **66 Deferred**: [คู่มือ](customer-identity-changes.md), [19-test combined regression](evidence/2026-09-20-customer-identity-tests.txt) และ [context](evidence/2026-09-20-customer-identity-context.json). Legal name/tax registration ใช้ before/after snapshot, independent review, reject/resubmit และ customer-version guard; apply เปลี่ยน master กับ immutable evidence ใน transaction เดียว. Address/contact identity, consent, external registry, browser/business acceptance ยังเหลือ.

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
