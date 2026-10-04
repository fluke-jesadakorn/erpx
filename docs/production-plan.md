# ERPX — ปิด engineering ทั้ง catalogue ก่อนนำเสนอ

ปรับปรุง 2026-10-03 · scope `catalogue-engineering-completion-v1` · Goal ต่อเนื่องตามคำสั่งผู้ใช้ · **ไม่ deploy staging หรือ production**

นี่คือแผน active ฉบับเดียว ปิดงานพัฒนาทั้ง **33 โมดูล / 187 capabilities** จาก `src/data/module-registry.json` แล้วจัด release candidate สำหรับสมัครงานหรือระดมทุน **ไม่รอผู้ตรวจรับภายนอก** ผล engineering แยกจาก business/legal/bank/device/provider production certification อย่างชัดเจน โค้ด หน้า CRUD เทสต์ที่มีอยู่ และ HTTP200 ไม่ใช่ผลตรวจครบ capability

ใช้บริษัทสมมติและบัญชีผู้ทำ ผู้อนุมัติ HR Finance Legal แยกกันผ่าน authentication ปกติ ใช้ Sandbox/provider ที่มีบัญชี และ adapter/fixtures ที่ระบุชัดสำหรับบริการที่ไม่มีบัญชี ไม่เพิ่ม OTA/SiteMinder, offline POS, registered e-Tax, autonomous AI หรือ roadmap ที่ประกาศเลื่อนไว้

## สถานะปัจจุบัน

ผลด้านล่างเป็น **engineering baseline ที่ตรวจวันที่ 2026-10-03 บน source/candidate ที่ระบุ**. [Durable receipt](testing/evidence/2026-10-03-engineering-completion.json) เก็บผลราย capability/gate และ original digests; [verification inventory](status/capability-delivery-matrix.md) แยก engineering/provider/Company release. การจัดเอกสารและตัวสร้างรายงานหลังรอบนี้ไม่ใช่การรันทดสอบทั้ง catalogue ใหม่ และไม่ให้ production/Company acceptance โดยอัตโนมัติ.

**ปิด 12/12 งานหลัก เหลือ 0 งานใน Goal นี้.** Final whole run `2026-10-03T10-45-58-035Z` ผ่าน **33/33 modules, 187/187 capabilities และ 12/12 required engineering gates** บน source/candidate เดียวกัน พร้อมชุดนำเสนอและผลตรวจ development ขั้นสุดท้ายด้านล่าง. ไม่ใช้ diagnostic หรือรอบที่ล้มเหลวแทนหลักฐานรอบนี้.

ตามคำสั่งผู้ใช้ **ยกเลิกเกณฑ์ ordinary read API p95 ≤500 ms** และรายงานค่าจริงแทน. คง 10 บัญชีต่อเนื่อง 30 นาที, content assertions, warm page median ≤1s, public navigation median ≤1.5s, zero 5xx/pool exhaustion/process restart และทุก gate อื่น.

## ฐานร่วมและลำดับทำงาน

1. **ผ่าน — snapshot เดิม:** `.scratch/dev/engineering-completion/2026-10-02T16-42-46Z/` เก็บ3,215 tracked/untracked files; HEAD `3fa064af7767debbb8a2a3f6085ced610f6286c5`; archive SHA256 `ff22b5f18b9d873048d1604a968826ad979b215a15d6af0cc8039fe3f04dfd7a`. Ignored secrets/runtime ไม่อยู่ใน archive. Registry ตรวจได้33/187จริง
2. **ผ่าน — scenario register/runner:** [engineering scenarios](testing/engineering-scenarios.json) ผูกแต่ละ capability กับ workflow/ผลที่ต้องได้/actual assertions. Runner fail เมื่อ scenario หาย ไม่ได้รัน required case skip หรือ source/build ต่างกัน
3. **ผ่าน — ฐานเงิน/กฎตามวันที่/File Sources/สิทธิ์:** ใช้ฐานร่วมก่อนเชื่อมปลายทาง ห้ามสร้างระบบเงินหรือไฟล์ซ้ำรายโมดูล
4. **ผ่าน — สามสายขนาน:** เงินและบัญชี / งานธุรกิจ / WorkplaceและPlatform; ผู้รวมงานดูแล interfaces/migrations/fingerprints/verification ใช้ implementation เดิมและเติมช่องว่างจริง
5. **ผ่าน — seed/source freeze/RC และ verification รวม:** source/build เดียว, DBแยก3104, scanner, compiled jobs, full integration/browser/upgrade/recovery/performance
6. **ผ่าน — ชุดนำเสนอ:** fictional demoทุกโมดูล/reset, report187พร้อมfingerprint/results/screenshots/reconciliation, quickstart/architecture/performance/video หลังgatesผ่าน

## Checklist ทั้ง33โมดูล

ทุก capability ต้องทำงานตั้งแต่เริ่มจนจบ รวมแก้ไข ยกเลิก ส่งต่อ ประวัติ/report และผลหลัง reload ตาม applicability ที่ระบุใน scenario ผล subset ไม่ยกระดับทั้ง module/capability

| Module | Capabilities | สถานะ | งานที่ต้องปิด |
| --- | ---: | --- | --- |
| Brand | 3 | ผ่าน | Draft/category locks/publish/history/rollback |
| Commercial | 5 | ผ่าน | Monthly meters/rating/shadow billing/invoice/grace/recovery (sandbox) |
| Identity & Access | 6 | ผ่าน | Principals/assignments/reviews/invite/recovery/revoke/conflicts/directory adapters |
| Organization | 4 | ผ่าน | Provisioning/tree/sharing/placement/node lifecycle/business units |
| Platform Admin | 9 | ผ่าน | Provision/connectors/infrastructure/migrations/dead letters/backup/restore/rollback/projections/audit/temporary access |
| Customer 360 | 4 | ผ่าน | Profile/notes/sales summary/timeline จาก source จริงตามสิทธิ์ |
| Customer Authority | 4 | ผ่าน | Identity/merge/bindings/duplicate review/access audit |
| Support | 6 | ผ่าน | Employee own-ticket; typed POS/PMS/asset sources; SLA/incidents/evidence/session/capability expiry/revoke |
| Finance | 9 | ผ่าน | Multi-line documents/posting/VAT/WHT/50ทวิ/เอกสารไทย/source adapters/many-item reconciliation/assets/depreciation/disposal/NPAEs notes/comparatives/year close; immutable exports |
| Point of Sale | 5 | ผ่าน | Currency/price snapshots/opening/corrections/refunds/Finance handoffs; restaurant/retail/room charge/continuous close; Stripe Sandbox |
| Sales | 5 | ผ่าน | Quote/order/partial delivery/cancellation/invoice/collection/return/FX refund |
| Assets | 5 | ผ่าน | Register/placement/condition/custody/retirement เชื่อม Finance |
| Inventory | 5 | ผ่าน | Split sourcing/UOM/partial receipts/issues/transfers/counts/lot/serial/reservations/quarantine/scrap/returns; quantity/value/GL ตรงกัน |
| Maintenance | 5 | ผ่าน | Preventive plans/work/downtime/block/resume/parts/cost/completion/history |
| Operations | 6 | ผ่าน | Typed exception queues/acknowledge/assign/escalate/review/resolve/versioned queue policies/source summaries |
| Procurement | 5 | ผ่าน | Requisition/RFQ/split sourcing/PO/partial receipt/invoice match/payment/supplier history |
| Quality | 4 | ผ่าน | Inspection/results/release gates/quarantine/scrap/rework/cancellation/evidence |
| Legal | 5 | ผ่าน | Typed privacy/workflows/intake/contracts/matters/obligations/DSAR/breach/holds/access/archive/source citations/portal/recovery; engineering UAT26/26 |
| People & Payroll | 5 | ผ่าน | Versioned effective-date Thai rules/source digest; เดือน/วัน/ชั่วโมง/OT/bonus/service charge/YTD/retro/final pay/slips/GL/bank files/results; adapter readiness แทน blanket block |
| CRM | 5 | ผ่าน | Accounts/activities/leads/opportunities → Sale/Stay/source history |
| Marketing | 6 | ผ่าน | Public forms → lead/content/media/domains/provider readiness/engagement |
| Manufacturing | 6 | ผ่าน | Routing/material/labor/output/scrap/quality/genealogy/WIP/cost/GL |
| Planning | 5 | ผ่าน | Demand จาก owner จริง/incoming supply/resource calendars/capacity/shortages/recommendations/scenario comparison |
| Projects | 6 | ผ่าน | WBS/dependencies/time/cost/graph/attachments/handoff ด้วย owner เดิม |
| E-commerce | 6 | ผ่าน | Channels/listings/carts/checkout/payment/fulfillment/returns กับ stock/Finance |
| Benefits | 5 | ผ่าน | Dependents/plan/window/election/provider/payroll; consume Learning eligibility จริง |
| Learning | 5 | ผ่าน | Catalog/assignments/attendance/results/certificates/expiry/completion/immutable handoff + consumption receipt |
| Talent | 6 | ผ่าน | Requisition/application/offer/proposals/hire → HR consume/reconcile ครั้งเดียว; privacy hold/erasure |
| Field Service | 6 | ผ่าน | Agreement/order/dispatch/visit/customer confirmation/exceptions/invoice/collection |
| Agents | 7 | ผ่าน | Approved prompt/source policy จริง/pools/replacement/private-session hold/expiry/reviewed proposals/invocation evidence |
| Collaboration | 8 | ผ่าน | Threads/mentions/comments/moderation/following/reactions/topics/retention/action cards/relations/attachments |
| Hotel PMS | 12 | ผ่าน | Reservation/front desk/rooms/housekeeping/folios/cashier/inventory/guests/reports/settings/opening/corrections/refunds/currency snapshots/night audit ต่อเนื่อง |
| ERPX Management | 4 | ผ่าน | Overview/billing/IAM/executive reports ตรงต้นทางและสิทธิ์ |

งานร่วม: Today/My work, Calendar/Inbox/Push, AI manual fallback, ภาษาและวันที่ของ Console, attachmentsทุกowner และperformance Work/People/PMS

## Interfaces และข้อมูลเดิม

- คง `/api/v1` และcommandsใต้โมดูลเจ้าของพร้อม `expectedVersion`, idempotency, transaction, audit/outbox และ current authority เพิ่ม lifecycle ที่ขาดตาม owner
- ADR-038: integer minor units/currency/scale/date/rate revision/fallback ที่immutable สำหรับทุกnew money write รวมreturns/payroll. Return อ้างsnapshotต้นฉบับและรักษายอดปัดเศษรวม Historical/backfill และเอกสาร/งบที่ออกแล้วคงความหมายเดิม
- Thai payroll ใช้versioned rule pack/effective dates/source digest ตรวจแหล่งราชการปัจจุบันเอง; reference calculation แยกจากengine เปลี่ยนblanket blockเป็นadapter/rules readiness แยกexternal certificationจากengineering
- Talent→HR และLearning→Benefits ใช้immutable handoffกับconsumption receiptแยก กันconsume/retry/reconcileซ้ำ ไม่เขียนทับต้นฉบับ
- Work/Social/Support/Collaboration/AI ใช้shared Source Record references ตรวจcurrent access/source versionใหม่ก่อนอ่านหรือใช้ผล
- Additive migrations พร้อมbackfill/upgrade/replay tests ห้ามdowngrade/deleteเงิน/เอกสาร/หลักฐาน/auditเพื่อให้gateผ่าน

## เกณฑ์จบและหลักฐาน

| Gate | สถานะ | ผลที่ต้องได้ |
| --- | --- | --- |
| Catalogue | ผ่าน | 33/33 modules,187/187 capabilities; missing/not-run/required skipped caseทำให้runnerล้มเหลว; testsเดิมใช้ซ้ำได้เมื่อassertionsพิสูจน์capabilityจริง |
| Integration | ผ่าน | Full isolated PostgreSQL suite; self-approval/revoke/cross-Company/stale/idempotency/concurrency/lost response/worker restartตามworkflow |
| Browser | ผ่าน | Desktop/mobileหลายบัญชีผ่านnormal login ตรวจworkflow/handoff/reloadจริงพร้อมscreenshots |
| Financial continuity | ผ่าน | เงิน/stock/subledger/GLตรงกัน; ปิดสองเดือนต่อเนื่อง/ปิดปี; exported versionsไม่ถูกเขียนทับ |
| Attachments | ผ่าน | Scanner/signatures/jobs/ทุกowner matrix/ownership/checksum/quota/frozen reconciliationผ่านก่อนเปิดglobal development gateพร้อมกัน |
| Legal | ผ่าน | Engineering UAT26/26; sources/citations/scope/recovery; ผลไม่ใช่business/legal/provider certification |
| Calendar/Inbox/Push/providers | ผ่าน | Recurrence/DST/reminder/current-source access/delivery/retry/revocationบนsandbox/fixtureที่ประกาศ |
| Performance | ผ่าน | Warm local page median≤1s; ordinary read API รายงาน p95 จริงโดยไม่กำหนดเพดานตายตัวตามคำสั่งผู้ใช้; public navigation median≤1.5s;10accounts/30minutesไม่มี5xx/pool exhaustion/memory restart |
| Build/upgrade/recovery | ผ่าน | Typecheck/lint0errors/full integration/browser/migration upgrade/encrypted backup-restore/rollbackบนsource/buildเดียวกัน |
| Presentation | ผ่าน | ReproducibleRC/seed/reset/report/screenshots/quickstart/architecture/performance/videoหลังgatesข้างต้นผ่าน |

## Environment และ availability

Development [Console dev](https://console-dev-erpx.jesadakorn.com/) → `127.0.0.1:3100`; isolated verification `3104` พร้อมDBแยก/scanner/compiled jobs. คงprivate production3101และPostgreSQL18.6ที่5432 ด้วยNode24.20.0/Bun1.2.17. ไม่แตะstaging3103/staging domains/erpx_stage_* หรือdeploy productionในGoalนี้ ไม่ใช้3000/3001/8080และไม่หยุดบริการอื่น

ก่อนจบcheckpointตรวจlocal/public developmentและprivate production `/api/health`ได้200 ตรวจaffected pageผ่านpublic Console devด้วยsessionปกติ และrecheck listenerหลังstartup. หากหยุดdevต้องคืนdetached process/log/PIDใต้ignored `.scratch/dev/`. Health200อย่างเดียวไม่ใช่UIverification

## ตัวนับ Goal — ปิด 12/12 tasks

ตัวหารตรึงที่ 12 ตามคำสั่งผู้ใช้; defect และการรันทดสอบซ้ำอยู่ใต้ task เดิม. ผลสุดท้ายยืนยันทั้ง implementation และ same-candidate gates ครบแล้ว.

- [x] EC-01 Payroll adjustment หลัง posted/paid, employee agreement/recovery และ daily/hourly จน GL/slip/bank
- [x] EC-02 Commerce foreign-currency goods, original functional valuation, return/credit/refund และ FX account controls
- [x] EC-03 Implement whole-cohort physical/transit/WIP GL และ SKU quantity/value continuity พร้อม native immutable original movement proofs
- [x] EC-04 Attachments 217 owner tuples และ final frozen development scan/checksum/quota reconciliation บน source สุดท้าย
- [x] EC-05 Final typecheck, lint และ full integration บน source ที่ตรึง
- [x] EC-06 Reproducible RC build, populated migration upgrade, encrypted eight-database restore และ compatible/incompatible rollback
- [x] EC-07 Fresh compiled desktop browser ครบ 76 required journeys ไม่มี missing/skip/retry
- [x] EC-08 Fresh compiled mobile browser ครบ 76 required journeys ไม่มี missing/skip/retry
- [x] EC-09 Same-build Legal 26/26 engineering UAT พร้อม current-source/download/worker/recovery evidence
- [x] EC-10 Continuous performance 10 distinct normal accounts / 30 minutes ตาม latency/error/process gates
- [x] EC-11 Complete 187-capability report, native 33-module guide/reset, screenshots, quickstart/architecture/performance และ concrete walkthrough video พร้อม source/candidate archives
- [x] EC-12 Stable development migrations/web/jobs, normal-auth public affected-page verification และ HTTP200 local/public dev/private production พร้อม PostgreSQL5432

## หลักฐานส่งมอบสุดท้าย

หลักฐานและชุดนำเสนออยู่ใน `.scratch/dev/engineering-completion/runs/2026-10-03T10-45-58-035Z/` (ignored/private local artifacts). รายงาน `completion.json` ระบุ complete:true, issues:[]; actual driver และทุก phase ออก 0. Source fingerprint `275ba4a6ae6f13a2fd72690969b7e307133afa1cfeebcd769f10b2aac474c097`, verification fingerprint `fbd63eb72f0ac4206f18ff42b2b4f8a7508d20156d19f4d2afaa4e78e354d20e`. Candidate `.runtime/releases/2026-10-03T10-46-43-224Z`, file-manifest SHA256 `48b9912ffc2b34a0ee4c82ba48033ffaf46c738562206885fce84586cb96b74c`.

| ผลตรวจ | ผลจริง |
| --- | --- |
| Typecheck/lint | ผ่าน, lint ไม่มี error |
| Full integration | 2,305 ผ่าน / 0 failed / 2 optional UCS live-provider checks skipped นอก required matrix; required native 491/491 identities ผ่านครั้งเดียวครบ ไม่มี required skip |
| Browser normal authentication | Desktop 76/76 และ mobile 76/76 ไม่มี missing/skip/unexpected/flaky; แยกผู้ทำ/ผู้อนุมัติและ reload; Legal UAT 26/26 ทั้งสอง project |
| เงิน/stock/WIP/GL | ทุก 3 Holdings, 7 Companies, 34 SKU ตรงกัน; 19 new movements และ original rows immutable; สองเดือนต่อเนื่อง/ปิดปี/immutable exports ผ่าน mapped Finance journeys |
| Attachments | 217 owner tuples / 225 unique native proofs ผ่าน; final ClamAV/checksum/ownership/quota ทั้ง Holding และ Billing ผ่าน; เก็บ 173 objects / 7,254,531 bytes เดิมครบ ไม่เดา owner หรือทำลายหลักฐาน; development upload เปิดหลังผ่าน |
| Migration/backup/restore/rollback | 17 recovery cases / 28 checks ผ่าน; encrypted restore 8 DB / 2,234 tables / 3 file stores; backfill once/replay zero; compatible rollback ผ่าน, incompatible rollback และ tampered ciphertext ถูกปฏิเสธ |
| Reproducible build | Fresh source copy 3,415 files + frozen install/actual build ผ่าน; 19 audit checks ผ่าน, 95 dependency versions / 1,361 canonical library files เท่ากัน; generated Next/path metadata differences บันทึกไว้ ไม่อ้าง literal-byte identical build |
| Performance | 10 normal accounts / 1,800,002 ms / 4,080 valid samples; same web/jobs PIDs, zero runtime errors/5xx/pool exhaustion/restart |

ค่าที่วัดจริง: API p95 Work **659.06 ms**, People **180.41 ms**, PMS **130.00 ms** (รายงาน ไม่มี fixed acceptance ceiling). Warm page median Work **188.66 ms**, People **180.56 ms**, PMS **581.00 ms**; public navigation median **124.10 ms**. ทั้ง page budgets และ continuous-run criteria ผ่าน.

Thai payroll และ Thai Finance rule sources/effective dates/source digests/reference calculations ตรวจแหล่งราชการและเก็บผลใน [Thai payroll rules](testing/thai-payroll-rules-2026-10-02.md) และ [Thai Finance tax rules](testing/thai-finance-tax-rules-2026-10-03.md). Source mapping/negative assertions ของทั้ง187 capabilities อยู่ใน [engineering scenarios](testing/engineering-scenarios.json). Provider flows ใช้ Sandbox ที่มีบัญชีและ declared fixtures/adapters ตาม mapping; external business/bank/device/provider certification แยกจาก engineering completion และไม่เป็นตัวขวาง Goal นี้.

ชุด `showcase/`: `33-modules.json`, `native-examples.json`, `187-capabilities.md`, screenshots, 430 original workflow clips, `walkthrough.mp4` (132.29 seconds, 18 actual-browser excerpt chapters พร้อม provenance), `quickstart.md` พร้อม normal-login demo/reset command, `architecture.md`, `performance.md`, exact-inventory `source.tar.gz` และ `candidate.tar.gz`. Root ตรวจภาพเปิด/ภาพ workflow จริง; playable H.264 1280×900 และ hashes/provenance ถูกตรวจ. `showcase-artifact-audit.json` ผูก artifact bytes/modes/source archive กับ current source. Private credentials/env/runtime ไม่อยู่ใน shareable source/guide.

ผล availability 2026-10-03 18:42 ICT: local dev3100/public dev/private production3101 `/api/health` **HTTP200 ทั้งสาม**, persistent pinned Bun1.2.17/Node24.20.0 dev/jobs พร้อม heartbeat ใหม่, uploads enabled/unfrozen, PostgreSQL18.6 port5432 พร้อมใช้. Read-only metadata ตรวจ development ทั้ง8 migration tracks ไม่มี pending/unrecognized checksum mismatch; exact recognized historical checksum exceptions เก็บไว้ ไม่แก้ receipts. Development migration table total2,225 แยกจาก recovery fixture2,234. Actual authenticated public [My work](https://console-dev-erpx.jesadakorn.com/holdings/acme/work) ตรวจภาพ/controls/typed cards และ reload แล้ว hydrated ใช้งานได้ โดยไม่ bypass login. ผลตรวจอยู่ใน `final-availability.json`, `final-http-availability.json`, `public-development-ui-receipt.json`.

Historical checkpoints, failed/interrupted run receipts และ original dirty snapshot เก็บใน ignored engineering-completion history; ไม่ restamp เป็นผล final และไม่มีการ reset working tree. แผนนี้เป็น active delivery plan ฉบับเดียว. ไม่มี staging access หรือ production deployment ในรอบนี้.
