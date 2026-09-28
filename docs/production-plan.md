# ERPX — แผน production ปัจจุบัน

ปรับปรุง 2026-09-26 · scope `thai-social-workplace-all-business` · **ยังไม่อนุมัติเปิดใช้จริง**

เอกสารนี้เป็นแผน active ฉบับเดียว ใช้แก้ไฟล์นี้ต่อเนื่อง ไม่สร้างแผนรายวันที่แข่งขันกัน ขอบเขตใหม่คือประสบการณ์ทำงานสำหรับทุกกลุ่มธุรกิจที่ ERPX รองรับ โดยเปิดใช้ทีละ flow เมื่อผ่านเกณฑ์ของ flow นั้น แผนโรงแรมไทยด้านล่างยังเป็นแทร็กตรวจรับธุรกิจเฉพาะ ไม่ใช่ขอบเขตทั้งหมดของผลิตภัณฑ์ วันที่ 1–10 ด้านล่างเป็น **10 วันทำงานสำหรับ checkpoint** ไม่ใช่กำหนดเปิดจริง ยังไม่มีโรงแรมและผู้ตรวจเงินเดือนไทยที่ระบุชื่อและรับงานแล้ว

## ขอบเขตใหม่: Today และ journey ของทุกฝ่าย

หน้าหลักเดียวชื่อ **Today** รวม Home และ Work ให้ตอบได้ทันทีว่า วันนี้ต้องทำอะไร ทีมกำลังทำอะไร และจะเริ่มเรื่องใหม่อย่างไร หัวหน้า Today ต้องสั้นและเห็นเนื้อหา Feed ในจอแรก เดสก์ท็อปใช้ Feed เป็นคอลัมน์หลัก และแถบขวาเรียง Needs you → Start something → My day โดยการ์ดสูงตามเนื้อหา เมื่อพื้นที่เนื้อหาต่ำกว่า 1,020px ให้ใช้คอลัมน์เดียวพร้อมสรุป Needs you และทางเปิด Plan my day ขนาดสั้นก่อน Feed; ปุ่ม Start ใน navigation เปิดทางเริ่มงานบนจอแคบ มุมมอง Feed, My actions, Tracking และ Team ใช้ Work adapters แสดงรายการจากต้นทางจริง ไม่สร้าง task สำเนาของคำขอซื้อ เบิกเงิน ลา หรือธุรกรรมอื่น รายการหนึ่งมีสถานะและคำสั่งเดียวกันทั้ง Today และแอปเจ้าของเรื่อง `/work` เดิมยังเปิด deep link ได้ระหว่างย้ายทางเข้า

เมนูหลักคือ Today, Calendar, Messages และ Apps พร้อม Search และ Start ที่เข้าถึงง่ายบนมือถือ เอา Operations ซึ่งกว้างเกินไปออก และให้ Projects & Routines กับแอปเจ้าของงานเป็นทางเข้ารายละเอียด ลดทางลัดซ้ำข้างหน้า ข้อมูลหน้าจอใช้ English และปี ค.ศ. เสมอ แม้ข้อความและเอกสารต้นฉบับเป็นภาษาไทย กฎไทยแยกจากภาษา UI โดยมี version และวันที่มีผลสำหรับกฎประเทศ นโยบายบริษัท และข้อยกเว้นที่ได้รับอนุญาต

| ระยะ | ขอบเขตที่ต้องตรวจรับ |
| --- | --- |
| 1 — Inventory และต้นแบบ | ทุก route/action ระบุผู้เริ่ม เจ้าของรายการ ผู้รับช่วง ผลสำเร็จ และทางแก้เมื่อผิดปกติ; ทดลอง Buy, Leave, Plan และ Post บนมือถือ/desktop กับผู้ใช้ใหม่ |
| 2 — พื้นฐานร่วม | Today, navigation, งานทั่วไปที่ไม่บังคับ deadline/approver, daily plan แยกจาก deadline/roster/attendance, audience ใหม่ทุกโพสต์, canonical form, AI guided intake flow แรก และ manual fallback |
| 3 — งานข้ามฝ่าย | Purchasing, Expense/Finance, Inventory, People, Projects, CRM/Support จบข้ามฝ่าย รวมส่งคืนแก้ ยกเลิก เปลี่ยนผู้รับผิดชอบ ทำบางส่วน และหลักฐานไม่ครบ |
| 4 — งานเฉพาะธุรกิจ | POS/restaurant, Hotel, Manufacturing, Maintenance, Field Service, E-commerce, Legal และฝ่ายสนับสนุน ผ่านการส่งต่อและผลกระทบเงิน/สต็อก/ความรับผิดชอบของแต่ละสาย |
| 5 — เปิดใช้อย่างควบคุม | Payroll ไทยที่ผู้เชี่ยวชาญตรวจ expected results อิสระ, Web Push แบบ opt-in, AI adapters สำหรับ flow ที่เปิด, คู่มือ/ข้อมูลตัวอย่าง, การติดตามและ rollback |

Social รองรับ Update, Plan, Blocker, Handover, Announcement, replies, mentions, Thanks และการเชื่อมรายการจริง ผู้โพสต์เลือก audience ใหม่ก่อนทุกการเผยแพร่และเห็น audience บนการ์ด การอ่าน กด Thanks หรือเลื่อนแผนไม่เปลี่ยนสถานะงาน โพสต์ที่แก้มีประวัติ และหากต้องขยายผู้ชมให้ถอนและเผยแพร่ใหม่ ข้อมูลค่าจ้าง สุขภาพ ธนาคาร เรื่องร้องเรียน วันลาและเอกสารส่วนตัวไม่เปิดเผยเพียงเพราะมี audience; post, comment, attachment, source preview, Search และ AI ตรวจสิทธิ์ต้นทางปัจจุบันเสมอ

ทุก flow เติมข้อมูลที่รู้แล้ว ถามเฉพาะที่ผู้เริ่มตอบได้ และแสดงช่องเฉพาะฝ่ายเมื่อส่งถึงฝ่ายนั้น AI คืน intent, draft, missing fields, sources และ actions ที่ทำได้ พร้อมร่างฟอร์มแก้ไขได้ แต่ไม่เดาราคา สิทธิ์ลา ผู้อนุมัติ หรือกฎภาษี และไม่สั่ง Publish/Submit/Approve/Post/Pay เอง ธุรกรรมที่ต้องแยกหน้าที่คง review อิสระ; งานทั่วไปสร้างและปิดเองได้ Daily update สมัครใจ เว้นแต่ handover/checklist ที่ทีมกำหนด ความสนุกมาจากการร่วมมือและการขอบคุณ ไม่จัดอันดับคนจากคะแนนหรือจำนวนโพสต์

**เกณฑ์ร่วมก่อนเปิดแต่ละ flow:** ผู้ใช้ใหม่อย่างน้อย 90% ทำงานตัวแทนสำเร็จโดยไม่มีคนบอกเมนู; Buy, Leave, Expense, Plan และ Post หาเจอภายในสองการกดจาก Today; ผู้ใช้บอกได้ว่าโพสต์ให้ใครเห็น งานอยู่กับใคร และต้องทำอะไรต่อ; พนักงาน หัวหน้า ผู้เชี่ยวชาญ และพนักงานกะผ่านมือถือ/desktop; refresh, กดซ้ำ, หลายแท็บ, เน็ตหลุด และ source version เก่าไม่ทำรายการซ้ำหรือทำร่างหาย; เปลี่ยนสิทธิ์/บริษัทแล้วข้อมูลไม่รั่ว; approval และบัญชีรักษาหลักฐาน version การแยกหน้าที่และการย้อนรายการ; AI คลุมเครือ/อ่านเอกสารผิด/เจอคำสั่งซ่อนยังมี manual path; วัดเวลา อัตราทำสำเร็จ งานค้างและการถามซ้ำ ไม่ใช้เวลาอยู่ในแอปหรือจำนวนโพสต์เป็นเป้าหมาย เป้าหมายคุณภาพ UI คือ WCAG 2.2 AA

**เกณฑ์เลย์เอาต์ Today:** ในสถานะเริ่มต้นที่ 1440×900 และ 390×844 เห็นหัวโพสต์/ประกาศแรกหรือ empty state โดยไม่เลื่อน; ตรวจ 320, 390, 768, 1024 และ 1440px กับ sidebar เต็ม/ย่อและ light/dark โดยไม่มี horizontal overflow; แถบขวาคงอยู่เมื่อเปลี่ยน Today view; สรุปจอแคบแสดงจำนวนงานระหว่างโหลด/ข้อมูลไม่ครบโดยไม่แสดงศูนย์ผิด และ `#plan`, `#compose`, `#handover` เปิดส่วนที่ถูกต้องพร้อมลำดับ keyboard/focus ที่อ่านตามหน้า

สถานะ implementation 2026-09-26: แกน Today/navigation, canonical Buy/Expense/Task, งานทั่วไปที่ไม่บังคับ deadline/review, daily plan, social audience/replies/revisions, ร่าง Handover จากงานจริงพร้อมยืนยันรับช่วง, guided intake แบบยืนยันก่อนใช้ร่าง, direct Support สำหรับบทบาท Support/Sales/owner และ Web Push แบบ opt-in มีโค้ดและการทดสอบเฉพาะแล้ว ตรวจหน้า Today, Handover และ Push Settings ที่ปิดอยู่บน dev ด้วย browser จริง **ยังไม่ใช่ผลตรวจรับทุกบทบาทหรือทุก flow** Push ยังไม่เปิดเพราะไม่มี VAPID/data keys; Front public dev origins ยังไม่ตอบสำหรับ browser QA. รายการ payroll ใน [สัญญาปัจจุบัน](contracts/next/payroll-core.md) ยังห้าม production calculation จน engine deterministic และชุด expected results ที่ผู้เชี่ยวชาญรับรองผ่าน Product owner ยืนยันว่ายังไม่มีผู้ตรวจรับ จึงคง gate นี้ไว้ ห้ามตีความการมีหน้าเว็บหรือ UAT เก่าว่า flow ใหม่ผ่านแล้ว

บัญชี journey และ route/action ที่ใช้ตรวจช่องว่างของทุกฝ่ายอยู่ใน [workplace journey inventory](journeys/2026-09-26-workplace-journey-inventory.md) โดยแยกเส้นทางที่มีจริงออกจากผลตรวจรับผู้ใช้จริง

## แทร็กข้ามระบบ: ไฟล์แนบและเอกสารหลักฐาน

ทุก workflow จริงในระยะ 1–4 ใช้ File Source, File Set และ File Link ชุดเดียวตาม [attachment contract](contracts/next/attachments.md) และ [ADR-036](adr/ADR-036-canonical-file-sources-and-policy-bound-links.md). Record documents เพิ่ม revision ได้ก่อน freeze; หลักฐานที่ผูกกับ submit, receipt, inspection, decision หรือ completion แล้วแก้หรือลบไม่ได้. Projection เช่น Today, report, stock balance และ ledger แสดง authorized source link เท่านั้น ไม่รับไฟล์เป็นเจ้าของเอง.

งานนี้พัฒนาภายใต้ `ERPX_ATTACHMENTS_V2` ที่ปิดไว้ และเป็นข้อยกเว้นจากการเปิดทีละ flow: Purchasing, Inventory, Sales, Finance/POS/PMS, Leave/People, Assets/Maintenance/Field Service, Manufacturing/Planning, Projects/Support, Legal/Admin และ authenticated portals ต้องผ่าน registry, permission, lifecycle, browser และ migration matrix พร้อมกันจึงเปิด gate ครั้งเดียว. ต้นฉบับทุกไฟล์เป็น private และ quarantine จน scanner ผ่าน; public product/storefront/marketing media ต้องเป็น derivative ที่อนุมัติพร้อม alt text และ revision.

Release gate ต้องมี `clamd`/signature updater ที่ healthy, signature ไม่เกิน 24 ชั่วโมง, legacy scan backlog เป็นศูนย์, owner/Company/purpose ของไฟล์เดิมครบหรือถูก quarantine อย่างชัดเจน และ object/checksum/quota reconciliation ผ่าน. Rollback ปิด reserve/bind ใหม่แต่คง clean bound reads และประวัติทั้งหมด; ห้ามลบหรือ downgrade หลักฐานเพื่อย้อน release.

## แทร็กแยก: Calendar และ staff Inbox

Calendar ส่วนตัวและ Inbox สำหรับพนักงานเป็นฐานของ Today และงานส่งมอบแยกจากเกณฑ์เปิดใช้โรงแรมไทย ขอบเขตปัจจุบันคือ Day/Week time grid, นัดหมายทั้งวันและซ้ำ, ผู้เข้าร่วมทุกบัญชีที่ active ใน Company, RSVP, free/busy, reminder ส่วนตัว, Inbox event/delivery ที่ตรวจสิทธิ์แหล่งข้อมูลปัจจุบัน และ notice แบบระบุชื่อหรือ broadcast ตามขอบเขตสิทธิ์ Web Push แบบ opt-in มี slice ในโค้ดและ [contract](contracts/next/web-push.md) แล้ว แต่ยังปิดและรอการตรวจ live browser/origin ในระยะ 5 ดู [Calendar contract](contracts/next/calendar.md), [Messaging/Inbox contract](contracts/next/messaging.md) และ [ADR-033](adr/ADR-033-company-scoped-staff-inbox-events.md).

สถานะ: กำลังพัฒนาและทดสอบแยกจาก pilot; ต้องผ่าน integration, DST/recurrence, current-access pagination, retry/reminder, และ browser สองบัญชีทั้ง desktop/mobile ก่อน cutover. บันทึกผลตรวจจริงและข้อค้างในแทร็กนี้ โดยไม่ยกระดับสถานะ release matrix โรงแรมไทยจากผลของ Inbox.

## แทร็กตรวจรับโรงแรมไทยและสถาปัตยกรรมเดิม

แทร็กโรงแรมไทยส่งมอบหนึ่งแห่งพร้อมร้านอาหาร จัดซื้อ HR เงินเดือนไทย และ AI เปิดชุดนี้เมื่อผ่านเกณฑ์ของแทร็กทั้งหมด บริษัทเดียว สถานประกอบการเดียว THB และกฎหมายไทย รวมพนักงานต่างชาติที่จ้างในประเทศไทย แทร็กนี้อยู่ภายใต้ journey matrix ทุกธุรกิจด้านบน ไม่ตีความจำนวน capabilities ใน catalog เป็นงานก่อนเปิดทั้งหมด

ใช้ Next.js/React/TypeScript, Node.js **24.20.0**, Bun **1.2.17**, PostgreSQL **18.6** และ durable PostgreSQL jobs ตาม [ADR-024](adr/ADR-024-local-postgresql-runtime.md) ไฟล์อัปโหลดและไฟล์ประกอบคอนฟิกผ่าน FileStore: dev ใช้ MinIO เดิมแล้ว ส่วน staging/production ยังรอ cutover จาก local files ตาม [ADR-035](adr/ADR-035-minio-object-and-configuration-boundary.md) คอนฟิกธุรกิจและ metadata อยู่ใน PostgreSQL; Cloudflare Tunnel เป็น ingress; Workers/D1/R2 และ Rust/Dioxus เป็นประวัติ ไม่ใช่ runtime ปัจจุบัน

ระหว่างพัฒนาให้ [public development](https://dev-erpx.jesadakorn.com) ใช้ `127.0.0.1:3100` และ [staging](https://erpx.jesadakorn.com) ใช้ `127.0.0.1:3103` คง private production service บน `3101` และ Tunnel rules อื่นไว้ การผ่าน release gates ไม่เปลี่ยน origin และไม่ใช่คำสั่งให้เปิดใช้งาน ต้องมีคำสั่งเปิดใช้ใหม่จากเจ้าของผลิตภัณฑ์

## เอกสารและความหมายของหลักฐาน

| แหล่ง | หน้าที่ |
| --- | --- |
| [README](../README.md), [TOC](TOC.md) | ทางเข้าและลิงก์ไปแผนนี้ |
| แผนนี้ | scope ลำดับงาน เจ้าของ เกณฑ์จบ หลักฐาน blocker และ checkpoint |
| [Contracts](contracts/next/README.md) | พฤติกรรมและ API ที่มีอยู่จริง; แยก planned จาก implemented |
| [CONTEXT](../CONTEXT.md), [ADR](adr/README.md) | คำศัพท์และเหตุผลการตัดสินใจ |
| [Evidence ledger](status/business-acceptance-evidence.json), `docs/testing/evidence/` | ข้อสังเกตพร้อมขอบเขตและ checksum; ผล subset ไม่รับรองทั้ง capability |
| [187-capability inventory](status/capability-delivery-matrix.md) | ทะเบียนอ้างอิง; จำนวน module/capability อ่านจาก registry ไม่คัดลอกตัวเลขหลายแห่ง |
| [ประวัติแผนและทะเบียนย้าย](archive/planning-2026-09/README.md) | แผนเก่า งานวิจัย รายงานตามเวลา และข้อกำหนดที่นำมาใช้ต่อ |

`Implemented` หมายถึงมี domain behavior ที่ persist แล้ว ไม่ได้แปลว่าผ่าน production หรือ statutory acceptance หน้าจอ CRUD, test file, demo หรือข้อความชื่อ module ไม่ใช่หลักฐานตรวจรับครบเส้นทาง

สถานะงานใช้ **พร้อมทำ / กำลังทำ / รอตรวจรับ / ผ่าน / ติดขัด / เลื่อน** เท่านั้น `ผ่าน` ต้องมีหลักฐานตรงเกณฑ์จบ เจ้าของในตารางเป็นบทบาทรับผิดชอบ ไม่ใช่การอ้างว่ามีบุคคลนั้นลงนามแล้ว

Gate ใช้ **`pending / passed / not_applicable`** แยกจากสถานะงานและ lifecycle `submitted/accepted/revoked` ของ setup; `not_applicable` ต้องมีเหตุผลและอนุญาตเฉพาะ provider inventory gate ที่ไม่ใช้ provider เกณฑ์ release ทุกข้อใน scope นี้จำเป็นทั้งหมดและต้อง `passed`

## Scope และ acceptance matrix

| ID / งาน | เจ้าของส่งมอบ → ผู้ตรวจรับ | เกณฑ์จบก่อนเปิดทั้งชุด | Baseline จากโค้ด / หลักฐาน | สถานะ |
| --- | --- | --- | --- | --- |
| DOC เอกสารและ scope | Engineering → Product owner | แผน active เดียว ลิงก์ใช้ได้ generator ตรงต้นทาง checksum เดิมผ่าน และข้อกำหนดเก่ามีปลายทาง | แผนนี้, [release contract](contracts/next/business-readiness.md), [ทะเบียนย้าย](archive/planning-2026-09/README.md) | กำลังทำ |
| GATE การรับรอง release | Engineering → QA/ผู้ตรวจแต่ละบทบาท | setup/test/business แยกกัน; PostgreSQL เก็บบริษัท scope/build/config/evidence/reviewer; สิทธิ์จริง version idempotency และ invalidate เมื่อเปลี่ยนค่าผ่าน tests | `src/server/release-acceptance.ts`, migration `0304`, `tests/release-acceptance.test.ts` | รอตรวจรับ |
| SCOPE ขอบเขตทุก execution path | Engineering → QA/IT | UI, API, owner services, workers และ AI ใช้ cut เดียวกัน; direct calls และคิวที่ค้างข้าม rollout ไม่หลุด scope รวม payroll | `src/data/production-scope.ts`, `src/server/hospitality-rollout.ts`; ยังต้องปิดข้อยกเว้น payroll และ audit owner/worker ทั้งหมด | กำลังทำ |
| HOTEL โรงแรม | PMS Engineering → ผู้จัดการโรงแรม/Finance | พนักงานจอง ห้อง/ราคา แม่บ้าน check-in folio checkout Night Audit; แก้ผิดได้โดยอนุมัติและบัญชีเชื่อมต้นทาง; browser หลายบัญชีและ reload ถูกต้อง | [PMS contract](contracts/next/pms.md), `src/server/pms.ts`, `tests/pms.test.ts`, [mock E2E acceptance matrix](testing/thai-hotel-v1-mock-e2e.md), [หลักฐานเดิม](testing/hospitality-pilot-implementation-2026-09-23.md) | รอตรวจรับ |
| CRM ลูกค้าและโอกาสขาย | Sales Engineering → Sales/ผู้จัดการโรงแรม | Prospect/Account/Contact และ Guest Link ผ่านการตรวจทาน; lead → Finance customer → ใบเสนอราคา/คำสั่งซื้อ หรือ lead → PMS reservation ที่ยืนยันแล้ว; แสดงการยกเลิก/no-show ภายหลังแยกจากวันที่ปิดดีล; funnel ตาม cohort, transition, conversion, loss reason, overdue, forecast, repeat business และยอดคาดการณ์/จอง/รายได้ posted แยกกัน | CRM/API, migration และ browser หลายบทบาท desktop/mobile พร้อม migration เดิม, ข้าม Company, reload และการนับไม่ซ้ำต้องตรวจครบ | กำลังทำ |
| MARKETING แหล่งที่มาและรายงาน | Marketing Engineering → Marketing/Finance | ฟอร์มสาธารณะ, การตรวจ/ส่งต่อ, Campaign ID และหลักฐานแหล่งที่มาจากพนักงานหรือการจอง; first touch กับ last touch แยกมุมมอง; ต้นทุนจาก Finance posted ที่เชื่อมชัด; ไม่มีหลักฐาน/ต้นทุนให้แสดงไม่มีข้อมูลแทน ROI | Marketing/API และ browser หลายบทบาท desktop/mobile, attributed/unattributed, posted costs และ reload ต้องตรวจครบ | กำลังทำ |
| POS ร้านอาหาร | POS Engineering → ผู้จัดการร้าน/Finance | โต๊ะ/เมนู ครัว สต็อก เงินสด โอนที่ตรวจ bank evidence ลงค่าบริการเข้าห้อง คืนเงิน ปิดกะ; ยอดเงินและสต็อกไม่ซ้ำ | [POS contract](contracts/next/pos.md), `src/server/pos-core.ts`, `tests/pos.test.ts`; ต้องตรวจ transfer refund/แก้หลัง checkout ครบเส้นทาง | รอตรวจรับ |
| BUY จัดซื้อ | Commerce Engineering → Purchasing/Finance | PR → RFQ เปรียบเทียบ → อนุมัติ PO → partial/full receipt → invoice matching → supplier payment → bank reconciliation; แยกผู้ทำ/อนุมัติ | `src/server/procurement-rfqs.ts`, [supplier payment](testing/supplier-payment-simulation.md), `tests/purchase-to-payment.test.ts`; simulation ไม่ใช่โรงแรมจริง | รอตรวจรับ |
| HR บุคลากร | People Engineering → HR | พนักงาน สัญญาจ้าง หัวหน้า สิทธิ์ กะ เวลา ลา OT เข้า–ออกงาน และ payroll handoff; HR เลือก Position ได้โดยไม่รอ IT ตั้งค่า GPO; ผู้ไม่มีสิทธิ์/ออกงานใช้แหล่งข้อมูลเดิมไม่ได้ | [HR/IT contract](contracts/next/hr-it-technology.md), [People lifecycle](contracts/next/people-lifecycle.md), [Roster](contracts/next/people-roster.md), [Self-service](contracts/next/staff-self-service.md) | รอตรวจรับ |
| HR-DEFAULT ค่าแนะนำ Acme dev | People Engineering → HR | Position มี pay default รายตำแหน่งและ Level มี leave default ราย version; HR ยืนยันต่อ Engagement, ค่าจ้างเป็น draft ให้คนอื่นอนุมัติ, การเปลี่ยน Level กลางปีเริ่มสิทธิวันลาใหม่ 1 มกราคมปีถัดไป; seed ซ้ำไม่ทับค่าที่แก้ | [Payroll](contracts/next/payroll-core.md), [Leave](contracts/next/staff-self-service.md), [seed](local-seed.md); fixture สมมติเฉพาะ Acme dev | รอตรวจรับ |
| PAY เงินเดือนไทย | Payroll Engineering → ผู้เชี่ยวชาญไทย/HR/Finance | deterministic country adapter ตามวันที่มีผล, gross-to-net, ภาษี/สมทบ/ยอดสะสม/ปัดเศษ/สลิป/GL/export/result/documents; independent expected results ครบกลุ่ม | [Payroll contract](contracts/next/payroll-core.md), `src/server/people-payroll.ts: createPayrollRun`; ปัจจุบัน production mode ปฏิเสธ `PAYROLL_ENGINE_UNAVAILABLE` มีเพียง gross preview กับ controlled GL/payment | ติดขัด |
| AI ผู้ช่วยสองเส้นทาง | AI Engineering → Purchasing/ผู้จัดการโรงแรม | ร่าง PR ให้คนยืนยัน และสรุปงานค้างก่อนปิดวันพร้อมแหล่ง/version/ขั้นตอน; provider ตามบริษัท source/access ตรวจซ้ำ quota/manual fallback | `src/server/request-ai.ts` มี PR proposals; day-close assistant ที่ครบเกณฑ์ยังไม่มีหลักฐาน | พร้อมทำ |
| BILLING Sandbox Gateway และ usage | Platform Engineering → IT/ผู้ดูแล ERPX/QA | AI ทุกทางผ่าน Gateway, meter ที่รวม active named seat ครั้งเดียวทั้ง Holding, provider statement กระทบยอด, ราคา versioned พร้อม hold ทั้งบิลเมื่อไม่ครบ, Stripe test invoice, สมุดบัญชีทดสอบแยก, browser ทุกบทบาท; ก่อนย้ายลูกค้าเดิมต้องมี shadow สองรอบ, margin และ capacity รวม backup พร้อม rollback staging | [ADR-037](adr/ADR-037-sandbox-ai-gateway-and-rated-holding-billing.md), [Billing contract](contracts/next/billing.md), [staging migration gate](../ops/macos/billing-target-staging-migration.md); 499 บาท/seat และ 2 GB/seat-month ยังเป็นสมมติฐาน Sandbox | รอตรวจรับ |
| MONEY ความถูกต้อง/สิทธิ์ | Domain owners → Finance/IT/QA | self-approval ถูกปฏิเสธ คำสั่งซ้ำ stale input revoked access cross-company bank mismatch และ worker ตายกลางงานไม่สร้างเงิน/stock ซ้ำ | tests ราย owner มีบางกรณี; ต้องจัด acceptance ครบ matrix ของ release นี้ | พร้อมทำ |
| RELEASE candidate | Engineering → QA | lint/typecheck/integration/isolated production build ผ่าน; upgrade และ browser หลายบัญชีใน production mode ผ่าน | ต้องบันทึกผลของ source build เดียวกันในหลักฐานท้ายไฟล์ ไม่ใช้ผลเก่าแทน | กำลังทำ |
| OPS ปฏิบัติการ | IT/Operations → ผู้รับผิดชอบโรงแรม | backup/restore อีกเครื่อง RPO ≤1h RTO ≤4h; rollback และ alert งานล้มเหลวจริง | [Recovery simulation](testing/recovery-simulation.md), [runbook](../ops/macos/README.md); off-machine drill ยังไม่มี | ติดขัด |
| BUSINESS เปิดจริง | Product owner → ผู้จัดการโรงแรม + HR/Payroll + Finance | โรงแรมจริงรับข้อมูลตั้งต้น ปิดวันกระทบยอด 2 วันติด และ payroll เทียบ reference ครบหนึ่งรอบ; ผู้รับผิดชอบลงนามตามหน้าที่ | ยังไม่มีโรงแรม บุคคล และข้อมูลอนุมัติจริง | ติดขัด |

**Checkpoint CRM/Marketing 2026-09-25:** migration `0310`/`0311` ใช้กับฐาน dev holding ทั้งสามแล้ว; integration ครอบคลุม legacy lead backfill แยก Account/Contact, Finance customer review, PMS win/cancellation/no-show, repeat stay, posted revenue, campaign touch/cost, public form consent/idempotency และสิทธิ์ข้าม Company (`tests/crm-migration.test.ts`, `tests/crm-marketing-analytics.test.ts`, `tests/public-marketing-form.test.ts`). Typecheck, lint (0 errors; 1 warning เดิม) และ isolated `deploy:build` ผ่าน. Browser dev แสดง CRM/Marketing สำหรับ Holding Owner ทั้ง desktop/mobile หลัง reload; บทบาทพนักงานถูกปฏิเสธหน้า Marketing. Staging ยังไม่มี session ที่ยืนยันตัวตนสำหรับตรวจหน้า และยังไม่มีข้อมูลโรงแรมจริงหรือผู้ตรวจรับ CRM/Marketing จึงไม่ถือว่าผ่านเกณฑ์ release.

รายการที่เลื่อน **เฉพาะแทร็กตรวจรับโรงแรมไทยเดิม** ได้แก่ OTA, public booking, offline POS, card/payment gateway, หลายโรงแรม, Executive canvas และการส่งอีเมล/provider webhook/journey/SMS/chat/loyalty ของ Marketing การกลับเข้าแทร็กโรงแรมต้องปรับผลกระทบและ acceptance ในไฟล์นี้ ส่วน Manufacturing, Marketing, Social และ AI guided intake อยู่ในขอบเขตผลิตภัณฑ์ทุกธุรกิจตามระยะ 2–4 ด้านบนแล้ว แต่ยังต้องผ่านเกณฑ์ของแต่ละ flow ก่อนเปิดใช้ โค้ดหรือหลักฐานเดิมไม่ขยายสถานะตรวจรับโดยอัตโนมัติ

## เงินเดือนไทยและข้อมูลที่ต้องรับ

PAY ครอบคลุมรายเดือน รายวัน รายชั่วโมง พนักงานต่างชาติภายใต้การจ้างในไทย เข้า–ออกกลางงวด โบนัส service charge OT ยอดสะสมระหว่างปี แก้ย้อนหลัง และสิ้นสุดการจ้าง **Contractor ใช้เจ้าหนี้ Finance** กับภาษีตามประเภทการจ่าย ไม่เปลี่ยน Contractor ให้เป็น employee payroll

Country adapter ต้องเป็น deterministic และ pin input digest, rule version, วันที่มีผล, currency scale และ rounding ใช้แหล่งราชการ เช่น [แนวคำนวณภาษีเงินเดือนของกรมสรรพากร](https://www.rd.go.th/3558.html) และตรวจประกาศที่ใช้ในงวดก่อนรับรอง ตรวจเปิดลิงก์นี้วันที่ 2026-09-24: เป็นคำสั่ง ป.96/2543 สำหรับการหักภาษีเงินได้จากการจ้าง ไม่รวมเงินได้ที่นายจ้างจ่ายครั้งเดียวเพราะออกจากงาน จึงยังต้องตรวจแหล่งทางการอื่นและประกาศที่มีผลในงวด; ไม่ใช่การรับรองกฎ/เพดานปัจจุบันครบชุด ห้าม AI คำนวณหรือเลือกกฎภาษี กรณีไม่รองรับต้องหยุดพร้อมเหตุผล ห้ามกรอกศูนย์แทนภาษีที่ยังไม่ทราบ

| ข้อมูล / งานปัจจัยภายนอก | เจ้าของ | เกณฑ์จบ | สถานะ |
| --- | --- | --- | --- |
| ยืนยันโรงแรม บริษัท/สาขา เลขภาษีและผู้รับผิดชอบ | Product owner | ระบุโรงแรมจริงและผู้จัดการ HR/Payroll Finance/IT พร้อมอำนาจลงนาม | ติดขัด |
| ผู้เชี่ยวชาญ payroll ไทย | Product owner / HR | แต่งตั้งผู้ตรวจรับที่ยอมรับชุด expected results และ rule versions ตามงวด | ติดขัด — Product owner ยืนยันว่ายังไม่มีผู้รับรอง; คง production calculation ปิดไว้ |
| Input และ expected results คำนวณอิสระ | HR/Payroll/Finance | ข้อมูลปกปิดตัวตนตามกลุ่มทั้งหมด; รายได้/หัก/สิทธิประโยชน์/ภาษีหักแล้ว/สมทบสะสม/วันทำงาน/OT/วันเริ่มสิ้นสุด; ค่าคาดหมายทุก output ตรวจได้ | ติดขัด |
| Payroll adapter และเอกสารราชการ | Payroll Engineering | calculation, YTD/correction/final-pay, payslip, GL, export และเอกสารภาษี/SSO ผ่านชุดผู้เชี่ยวชาญโดยไม่มี AI | พร้อมทำ |
| รูปแบบธนาคารและการยื่นแบบ | Finance | ตัวอย่างไฟล์และผลรับ/ปฏิเสธที่ผู้รับผิดชอบยืนยัน พร้อม reconciliation | ติดขัด |
| เครื่อง restore และผู้ดูแล | Operations | ระบุเครื่องอีกเครื่อง storage/network access, drill ที่จับเวลาและตรวจ business reconciliation | ติดขัด |

ERPX เตรียมข้อมูลยื่นแบบ/จ่ายและเก็บผลยืนยัน ผู้รับผิดชอบยื่นและโอนเงินจริงภายนอก การคืนเงินโอนและแก้บิลหลัง checkout ต้องมีอนุมัติ หลักฐาน และรายการบัญชีเชื่อมต้นทาง

## ลำดับดำเนินการและ checkpoint

| ช่วง | งาน / เจ้าของ | เกณฑ์จบ | สถานะ |
| --- | --- | --- | --- |
| วันที่ 1–2 | DOC/GATE baseline, scope/acceptance matrix, ขอข้อมูล PAY/BUSINESS — Engineering/Product owner | แผนเดียว baseline จากโค้ดและ blocker ที่ระบุเจ้าของ | กำลังทำ |
| วันที่ 3–4 | GATE/SCOPE, isolated production rehearsal และ gap review HOTEL/POS/BUY/HR — Engineering/QA | replay/version/revocation/config drift/migration tests และ gap list ที่ผูกเส้นทาง | พร้อมทำ |
| วันที่ 5–8 | MONEY → HOTEL/POS/BUY/HR → PAY → AI — Domain owners | จบเส้นทางหนึ่งรวม correction/retry/acceptance ก่อนเริ่มเส้นทางถัดไป | พร้อมทำ |
| วันที่ 9–10 | สาธิตหลายบทบาทและออก checkpoint — QA/Product owner | ระบุผ่าน/ไม่ผ่าน/ติดปัจจัยภายนอกจากหลักฐาน; ประมาณงานที่เหลือโดยไม่แปลงเป็นวันเปิด | พร้อมทำ |
| หลัง checkpoint | ทำรายการเดิมต่อและรับรองกับโรงแรมจริง — ทุก owner | ทุก release gate ผ่านบน build/config/scope เดียวกัน; ได้คำสั่งเปิดใช้ | ติดขัด |

ทุก flow ที่มีผลต่อเงิน สิทธิ์ และข้อมูลต้องผ่านการตรวจรับเฉพาะด้านก่อนเปิดใช้ งานในขอบเขต Today ทุกธุรกิจได้รับการตัดสินใจแล้วตามส่วนแรก; หากขยายเกินขอบเขตนี้จึงปรับ scope version และเกณฑ์ในแผน

## ข้อกำหนดเดิมที่คงใช้

- UI ต้องบอกข้อมูลจริง/preview และ manual fallback ชัด; permission-aware navigation และการซ่อนเมนูไม่แทน owner authorization; scope สืบต่อผ่านลิงก์และคำสั่ง
- Person, Engagement, principal, position, level และ business role เป็นคนละความหมาย ไม่มี ERPX role หรือความอาวุโสที่อนุมานจากชื่อตำแหน่ง; IT อาจผูก GPO กับ Position และงาน Directory จะตามตำแหน่งที่มีผล โดยไม่กั้น HR หรือให้ ERPX role อัตโนมัติ
- HR กำหนด `rosterRequired` ที่ Position โดยค่าเริ่มต้นเป็น backoffice; การเปิดใช้งาน Engagement ยังต้องเลือก Position ตามกฎเดิม ผู้เข้ากะต้องมี assignment อย่างน้อยหนึ่งกะต่อสัปดาห์ที่ทำงานก่อนเผยแพร่ ยกเว้นลาอนุมัติเต็มสัปดาห์ ผู้จัดการ backoffice ยังจัดการ Roster ได้ตามสิทธิ์ แต่ตนเองไม่ต้องมี Open Slot
- Spending readiness ตรวจตาม request: reporting chain ที่ใกล้และมีสิทธิ์, Accounting reviewer ที่แยกจากผู้ทำและ requester, Finance payment พร้อมหลักฐาน, partial payments คงยอดค้าง และ rerouting ต้องตรวจ record/source versions
- Domain owner รับผิดชอบยอดเงิน/stock/ledger lifecycle; generic record CRUD ไม่แทนธุรกรรม Approval, reversal, correction, idempotency, audit/outbox และ private evidence ต้องอยู่ด้วยกัน
- AI เป็นข้อเสนอเท่านั้น ผูก provider/source/config/version ตรวจสิทธิ์ตอนอ่านและก่อน apply; ไม่ submit/approve/post/pay; manual input และ draft ไม่หายเมื่อ provider ล่ม
- Workplace ใช้ keyboard/mobile ได้ รักษา scope, identity และ private evidence; retained contracts อยู่ที่ [workplace UX](contracts/next/workplace-ux.md), [staff work](contracts/next/staff-work.md), [identity](contracts/next/identity-access.md), [HR/IT](contracts/next/hr-it-technology.md), [HR lifecycle](contracts/next/people-lifecycle.md), [Finance](contracts/next/finance.md)
- Security/privacy/accessibility และหลักฐานย้อนหลังยังใช้ต่อ Social ใน Today เป็นขอบเขตระยะ 2 แล้ว ส่วน full catalog, public beta, Executive canvas และ Cloudflare Workers deployment จากแผนเก่าเป็นประวัติหรือเลื่อน ไม่ใช่เกณฑ์เปิดใช้เพิ่มเติม

## ผลรอบ implementation นี้

Baseline commit `e1ab884a`; เริ่มจาก working tree ที่มีงาน Executive/POS/front shell/production ค้างอยู่ก่อนแล้ว รักษางานเดิมไว้ ไม่ถือว่าการทดสอบรวมทั้งหมดเป็นหลักฐานเฉพาะ patch นี้ ผลการตรวจและข้อจำกัดจะอัปเดตในส่วนนี้เมื่อ verification จบ

- Gate generation ใช้ต้นทาง review แยกจาก generated artifacts; รายการว่างยังคง pending ไม่ยกระดับจาก observations
- PostgreSQL release acceptance แยกจาก Company setup; API ใหม่ใต้ business-readiness ใช้สิทธิ์เฉพาะเกณฑ์, independent reviewer, optimistic version, idempotency, immutable evidence/events และ material-configuration invalidation
- Release build ผูก fingerprint ของ source/schema/toolchain/public build configuration; dev ใช้ `unbuilt` และรับรอง production ไม่ได้
- Verification 2026-09-24: migration ใช้กับฐาน local ทั้ง 8 ชุดผ่าน, release acceptance 7/7 tests ผ่าน (รวม superseded release), ชุดเกี่ยวข้องก่อนแก้ superseded 115/115 ผ่าน, lint ไม่มี error (มี warning เดิม 1 จุด), typecheck และ isolated production build ผ่าน; integration suite ทั้งชุดถูกหยุดหลังรัน 15 นาทีโดยยังไม่แสดงผลสรุป จึง **ไม่ถือว่าผ่าน** และต้องรันจนจบก่อนให้ release gate ผ่าน
- Browser ของผู้ใช้ที่ public hostname แสดง 13 release criteria เป็น pending พร้อม blocker จริง; local/public `/api/health` ตอบ 200 ระหว่างตรวจ และ dev ยังคงอยู่ที่ 3100
- Existing payroll production prohibition ยังไม่ถูกยกเลิก: statutory adapter ไม่ได้ติดตั้งและไม่มี independent Thai fixtures จึงยังไม่สามารถผ่าน PAY หรือเปิดทั้งชุด งานนี้คงอยู่ใน matrix จนจบ
- ไม่มีการรับรองธุรกิจจริง ไม่มีการเปลี่ยน public origin และไม่มีการ deploy ไป service 3101
