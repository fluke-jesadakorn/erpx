# Acme staffing and workflow test data — 2026-09-14

> Recorded report — findings, counts, gaps and hostname observations below belong to the original run. Current workflows: [owner catalogue](../contracts/next/catalogue.md); latest recorded engineering/provider/release boundaries: [verification inventory](../status/capability-delivery-matrix.md). This report is retained evidence, not an active plan.

เพิ่มพนักงานจำลอง **Acme Corporation บริษัทเดียว 92 คน** รวมกับเดิม 20 คนเป็น **112 โปรไฟล์** ใน Bangkok และ New York เก็บโปรไฟล์เดิม สายรายงานเดิม สิทธิ์เดิม และข้อมูล Beacon / Northstar ไว้ทั้งหมด

## โครงสร้าง

ทั้งสองสาขามี Operations, Finance / Treasury, People / HR, Accounting, Sales, Purchasing, Warehouse / Logistics, IT และ Customer Service รวม 18 หน่วยแผนก

แต่ละแผนกเพิ่ม Manager (Level 3), Supervisor (Level 4), Senior (Level 5) และ Staff สองคน (Level 6) รวม 5 คน มี Branch Director (Level 2) สาขาละหนึ่งคนขึ้นตรงกับ Mali Srisai ซึ่งเป็น CEO เดิม (Level 1)

สายรายงานใหม่: CEO → Branch Director → Department Manager → Supervisor → Senior / Staff โดยใช้ Reports to จริง Staff สามารถรายงานข้าม Level 5 ไปยัง Supervisor ได้

| Level | เพิ่มใหม่ | รวมใน Acme หลังนำเข้า |
|---|---:|---:|
| 1 CEO | 0 | 1 |
| 2 Executive / Director | 2 | 3 |
| 3 Manager | 18 | 21 |
| 4 Supervisor | 18 | 19 |
| 5 Senior | 18 | 19 |
| 6 Staff | 36 | 49 |
| **รวม** | **92** | **112** |

## ล็อกอินและสิทธิ์

บัญชีใหม่ทั้ง 92 คนล็อกอินได้ อีเมลปัจจุบันใช้ `firstname.lastname@erpx.dev` และรหัสผ่านทดสอบใช้ `12345678` ตามคำขอผู้ใช้เมื่อ 2026-09-15 การรัน seed ซ้ำไม่เปลี่ยนรหัสผ่าน ไม่คืนสิทธิ์ที่ HR เพิกถอน และไม่เขียนทับการแก้โปรไฟล์

เริ่มจาก [หน้าเข้าสู่ระบบ](https://erpx.jesadakorn.com/login) แล้วเลือก workspace ที่มีสิทธิ์ พนักงานทั่วไปใช้ workspace แผนกของตน บัญชี HR ระดับบริษัทใช้ People และ Organization ได้ทั้ง Acme

| หน้าที่ทดสอบ | บุคคล | อีเมล |
|---|---|---|
| Requester, Level 6 | Elena Suwan | elena.suwan@erpx.dev |
| Supervisor, Level 4 | Clara Suwan | clara.suwan@erpx.dev |
| Branch Director, Level 2 | Anya Suwan | anya.suwan@erpx.dev |
| Accounting preparer, Level 5 | Seth Suwan | seth.suwan@erpx.dev |
| Accounting approver, Level 4 | Rina Suwan | rina.suwan@erpx.dev |
| Independent GL poster, Level 3 | Quinn Suwan | quinn.suwan@erpx.dev |
| Treasury payer, Level 6 | Jade Suwan | jade.suwan@erpx.dev |
| Company HR, Level 3 | Lara Suwan | lara.suwan@erpx.dev |

เลือกผู้รับผิดชอบเหล่านี้ใน flow ของเอกสารเพื่อทดลอง **Elena → Clara → Seth → Rina → Quinn → Jade** มีผู้ปฏิบัติงานสำรองใน New York สำหรับคำขอที่ Accounting / Finance เป็น requester เอง คำขอของ CEO ข้ามเฉพาะ Supervisor

กำหนดบทบาทแยกจาก Level อย่างชัดเจน: Accounting preparer 4 คน, Accounting approver 4 คน (Level 3–4), Treasury 8 คน โดยไม่ซ้อนบทบาทเตรียมบัญชี / อนุมัติบัญชี / จ่ายเงินในบุคคลใหม่คนเดียวกัน มี purchasing, sales, warehouse, HR, payroll และ IT ตามหน้าที่ด้วย สิทธิ์ใหม่หมดอายุ 2027-09-14 UTC ไม่มีการเพิ่ม holding owner หรือ platform role

employee และ people_manager จำกัดตามแผนก/สาขา บทบาทบริการส่วนกลาง เช่น Accounting, Treasury และ company HR ครอบคลุมบริษัท Acme การเป็นหัวหน้าหรือมีบทบาทส่วนกลางไม่เพิ่มสิทธิ์อ่านห้องสนทนาของแผนกอื่น สมาชิก organization channels ยังอิง placement ปัจจุบัน

## กรณีที่ควรทดลอง

1. Requester ทุก Level ส่ง expense, purchase และ payment request แล้วตรวจผู้รับผิดชอบแต่ละขั้น
2. Accounting หรือ Finance ส่งคำขอของตัวเอง และเลือกคนสำรองให้ครบโดยไม่อนุมัติหรือจ่ายรายการตัวเอง
3. Level 4 อนุมัติ Accounting ได้ ส่วน Level 5 ไม่ผ่านเกณฑ์ แม้ได้รับ finance_approver เพิ่มชั่วคราวในกรณีทดสอบ
4. Return พร้อมเหตุผล แก้สาระสำคัญ แล้วส่งอนุมัติใหม่ ทดสอบ purchase ก่อนสั่งซื้อ รับสินค้า ตรวจ invoice และ posting ก่อนบันทึก payment พร้อม slip รวม partial payment
5. HR ย้ายสาขา เปลี่ยน Reports to หรือระงับผู้อนุมัติ แล้วตรวจ channel membership และ explicit route repair ของเอกสารเก่า
6. ล็อกอินพนักงานทั่วไปและตรวจว่าเปิด workspace บริษัทหรือแผนกอื่นไม่ได้

ชุดนี้เตรียมข้อมูลคน บัญชี และสิทธิ์ เอกสารทดสอบ เงินเดือน และรายการจ่ายเงินจริงไม่ได้ถูกสร้างโดย seed นี้ เพิ่มข้อมูลเหล่านั้นผ่าน flow ที่ต้องการทดสอบ โดย payment เป็นการบันทึกหลักฐานการจ่ายภายนอก ERPX

## รันซ้ำและตรวจสอบ

```bash
bun run seed:organization:preview  # อ่านแผนก่อนทำงาน
bun run seed:organization:acme     # เพิ่มเฉพาะรายการที่ยังไม่มี
bun run seed:organization:verify   # ตรวจอย่างเดียว ไม่เขียนฐานข้อมูล
bun run test:integration tests/unit/organization/acme-organization-fixture.test.ts tests/unit/organization/organization-chart.test.ts
bun run test:e2e:dev tests/e2e/organization/acme-organization-data.spec.ts tests/e2e/organization/organization-chart.spec.ts
```

จำกัดการทำงานกับ local development PostgreSQL บน 5434 และ database prefix `erpx_dev_` เท่านั้น ไม่รวมไว้ใน `seed:local` โดยอัตโนมัติ ต้องมี CEO, Level catalog และสาขาเดิมที่ตั้งไว้แล้ว

บันทึกข้อมูลก่อนเปลี่ยนและแผนรายการใหม่ไว้ใน ignored `.scratch/acme-organization/before-*/` ทุกการสร้างคนและสิทธิ์มี audit history การตรวจใช้ domain readiness จริงของพนักงานใหม่ 92 คนและ CEO × 3 ประเภทคำขอ = **279 กรณี** ตรวจ reporting graph และ channel eligibility ด้วย รายงานอยู่ใน `.scratch/acme-organization/verification.json` การตรวจ readiness ไม่สร้างเอกสารหรือทำรายการจ่าย

## รายชื่อบัญชีใหม่ทั้งหมด

รหัสผ่านเริ่มต้นของทุกรายชื่อใช้สูตรข้างต้น ตารางนี้เป็นข้อมูลจำลองสำหรับ development เท่านั้น

| สาขา / แผนก | ชื่อ | Level | Reports to | Login |
|---|---|---:|---|---|
| bangkok / operations | Anya Suwan | 2 | Mali Srisai | anya.suwan@erpx.dev |
| bangkok / operations | Ben Suwan | 3 | Anya Suwan | ben.suwan@erpx.dev |
| bangkok / operations | Clara Suwan | 4 | Ben Suwan | clara.suwan@erpx.dev |
| bangkok / operations | Dev Suwan | 5 | Clara Suwan | dev.suwan@erpx.dev |
| bangkok / operations | Elena Suwan | 6 | Clara Suwan | elena.suwan@erpx.dev |
| bangkok / operations | Finn Suwan | 6 | Clara Suwan | finn.suwan@erpx.dev |
| bangkok / finance | Gia Suwan | 3 | Anya Suwan | gia.suwan@erpx.dev |
| bangkok / finance | Hana Suwan | 4 | Gia Suwan | hana.suwan@erpx.dev |
| bangkok / finance | Ivan Suwan | 5 | Hana Suwan | ivan.suwan@erpx.dev |
| bangkok / finance | Jade Suwan | 6 | Hana Suwan | jade.suwan@erpx.dev |
| bangkok / finance | Kai Suwan | 6 | Hana Suwan | kai.suwan@erpx.dev |
| bangkok / people | Lara Suwan | 3 | Anya Suwan | lara.suwan@erpx.dev |
| bangkok / people | Mason Suwan | 4 | Lara Suwan | mason.suwan@erpx.dev |
| bangkok / people | Nora Suwan | 5 | Mason Suwan | nora.suwan@erpx.dev |
| bangkok / people | Omar Suwan | 6 | Mason Suwan | omar.suwan@erpx.dev |
| bangkok / people | Pim Suwan | 6 | Mason Suwan | pim.suwan@erpx.dev |
| bangkok / accounting | Quinn Suwan | 3 | Anya Suwan | quinn.suwan@erpx.dev |
| bangkok / accounting | Rina Suwan | 4 | Quinn Suwan | rina.suwan@erpx.dev |
| bangkok / accounting | Seth Suwan | 5 | Rina Suwan | seth.suwan@erpx.dev |
| bangkok / accounting | Tara Suwan | 6 | Rina Suwan | tara.suwan@erpx.dev |
| bangkok / accounting | Uri Suwan | 6 | Rina Suwan | uri.suwan@erpx.dev |
| bangkok / sales | Vera Suwan | 3 | Anya Suwan | vera.suwan@erpx.dev |
| bangkok / sales | Wade Suwan | 4 | Vera Suwan | wade.suwan@erpx.dev |
| bangkok / sales | Xena Suwan | 5 | Wade Suwan | xena.suwan@erpx.dev |
| bangkok / sales | Yara Suwan | 6 | Wade Suwan | yara.suwan@erpx.dev |
| bangkok / sales | Zane Suwan | 6 | Wade Suwan | zane.suwan@erpx.dev |
| bangkok / purchasing | Ada Suwan | 3 | Anya Suwan | ada.suwan@erpx.dev |
| bangkok / purchasing | Bryn Suwan | 4 | Ada Suwan | bryn.suwan@erpx.dev |
| bangkok / purchasing | Cleo Suwan | 5 | Bryn Suwan | cleo.suwan@erpx.dev |
| bangkok / purchasing | Dara Suwan | 6 | Bryn Suwan | dara.suwan@erpx.dev |
| bangkok / purchasing | Evan Suwan | 6 | Bryn Suwan | evan.suwan@erpx.dev |
| bangkok / warehouse | Fern Suwan | 3 | Anya Suwan | fern.suwan@erpx.dev |
| bangkok / warehouse | Gavin Suwan | 4 | Fern Suwan | gavin.suwan@erpx.dev |
| bangkok / warehouse | Hazel Suwan | 5 | Gavin Suwan | hazel.suwan@erpx.dev |
| bangkok / warehouse | Ida Suwan | 6 | Gavin Suwan | ida.suwan@erpx.dev |
| bangkok / warehouse | Jonah Suwan | 6 | Gavin Suwan | jonah.suwan@erpx.dev |
| bangkok / it | Kira Suwan | 3 | Anya Suwan | kira.suwan@erpx.dev |
| bangkok / it | Leon Suwan | 4 | Kira Suwan | leon.suwan@erpx.dev |
| bangkok / it | Mira Suwan | 5 | Leon Suwan | mira.suwan@erpx.dev |
| bangkok / it | Nolan Suwan | 6 | Leon Suwan | nolan.suwan@erpx.dev |
| bangkok / it | Opal Suwan | 6 | Leon Suwan | opal.suwan@erpx.dev |
| bangkok / customer-service | Paolo Suwan | 3 | Anya Suwan | paolo.suwan@erpx.dev |
| bangkok / customer-service | Reva Suwan | 4 | Paolo Suwan | reva.suwan@erpx.dev |
| bangkok / customer-service | Soren Suwan | 5 | Reva Suwan | soren.suwan@erpx.dev |
| bangkok / customer-service | Tessa Suwan | 6 | Reva Suwan | tessa.suwan@erpx.dev |
| bangkok / customer-service | Victor Suwan | 6 | Reva Suwan | victor.suwan@erpx.dev |
| new-york / operations | Anya Bennett | 2 | Mali Srisai | anya.bennett@erpx.dev |
| new-york / operations | Ben Bennett | 3 | Anya Bennett | ben.bennett@erpx.dev |
| new-york / operations | Clara Bennett | 4 | Ben Bennett | clara.bennett@erpx.dev |
| new-york / operations | Dev Bennett | 5 | Clara Bennett | dev.bennett@erpx.dev |
| new-york / operations | Elena Bennett | 6 | Clara Bennett | elena.bennett@erpx.dev |
| new-york / operations | Finn Bennett | 6 | Clara Bennett | finn.bennett@erpx.dev |
| new-york / finance | Gia Bennett | 3 | Anya Bennett | gia.bennett@erpx.dev |
| new-york / finance | Hana Bennett | 4 | Gia Bennett | hana.bennett@erpx.dev |
| new-york / finance | Ivan Bennett | 5 | Hana Bennett | ivan.bennett@erpx.dev |
| new-york / finance | Jade Bennett | 6 | Hana Bennett | jade.bennett@erpx.dev |
| new-york / finance | Kai Bennett | 6 | Hana Bennett | kai.bennett@erpx.dev |
| new-york / people | Lara Bennett | 3 | Anya Bennett | lara.bennett@erpx.dev |
| new-york / people | Mason Bennett | 4 | Lara Bennett | mason.bennett@erpx.dev |
| new-york / people | Nora Bennett | 5 | Mason Bennett | nora.bennett@erpx.dev |
| new-york / people | Omar Bennett | 6 | Mason Bennett | omar.bennett@erpx.dev |
| new-york / people | Pim Bennett | 6 | Mason Bennett | pim.bennett@erpx.dev |
| new-york / accounting | Quinn Bennett | 3 | Anya Bennett | quinn.bennett@erpx.dev |
| new-york / accounting | Rina Bennett | 4 | Quinn Bennett | rina.bennett@erpx.dev |
| new-york / accounting | Seth Bennett | 5 | Rina Bennett | seth.bennett@erpx.dev |
| new-york / accounting | Tara Bennett | 6 | Rina Bennett | tara.bennett@erpx.dev |
| new-york / accounting | Uri Bennett | 6 | Rina Bennett | uri.bennett@erpx.dev |
| new-york / sales | Vera Bennett | 3 | Anya Bennett | vera.bennett@erpx.dev |
| new-york / sales | Wade Bennett | 4 | Vera Bennett | wade.bennett@erpx.dev |
| new-york / sales | Xena Bennett | 5 | Wade Bennett | xena.bennett@erpx.dev |
| new-york / sales | Yara Bennett | 6 | Wade Bennett | yara.bennett@erpx.dev |
| new-york / sales | Zane Bennett | 6 | Wade Bennett | zane.bennett@erpx.dev |
| new-york / purchasing | Ada Bennett | 3 | Anya Bennett | ada.bennett@erpx.dev |
| new-york / purchasing | Bryn Bennett | 4 | Ada Bennett | bryn.bennett@erpx.dev |
| new-york / purchasing | Cleo Bennett | 5 | Bryn Bennett | cleo.bennett@erpx.dev |
| new-york / purchasing | Dara Bennett | 6 | Bryn Bennett | dara.bennett@erpx.dev |
| new-york / purchasing | Evan Bennett | 6 | Bryn Bennett | evan.bennett@erpx.dev |
| new-york / warehouse | Fern Bennett | 3 | Anya Bennett | fern.bennett@erpx.dev |
| new-york / warehouse | Gavin Bennett | 4 | Fern Bennett | gavin.bennett@erpx.dev |
| new-york / warehouse | Hazel Bennett | 5 | Gavin Bennett | hazel.bennett@erpx.dev |
| new-york / warehouse | Ida Bennett | 6 | Gavin Bennett | ida.bennett@erpx.dev |
| new-york / warehouse | Jonah Bennett | 6 | Gavin Bennett | jonah.bennett@erpx.dev |
| new-york / it | Kira Bennett | 3 | Anya Bennett | kira.bennett@erpx.dev |
| new-york / it | Leon Bennett | 4 | Kira Bennett | leon.bennett@erpx.dev |
| new-york / it | Mira Bennett | 5 | Leon Bennett | mira.bennett@erpx.dev |
| new-york / it | Nolan Bennett | 6 | Leon Bennett | nolan.bennett@erpx.dev |
| new-york / it | Opal Bennett | 6 | Leon Bennett | opal.bennett@erpx.dev |
| new-york / customer-service | Paolo Bennett | 3 | Anya Bennett | paolo.bennett@erpx.dev |
| new-york / customer-service | Reva Bennett | 4 | Paolo Bennett | reva.bennett@erpx.dev |
| new-york / customer-service | Soren Bennett | 5 | Reva Bennett | soren.bennett@erpx.dev |
| new-york / customer-service | Tessa Bennett | 6 | Reva Bennett | tessa.bennett@erpx.dev |
| new-york / customer-service | Victor Bennett | 6 | Reva Bennett | victor.bennett@erpx.dev |
