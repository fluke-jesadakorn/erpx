# บริษัท Demo ที่ใช้ workflow จริง

ERPX มี environment จำลองแบบทิ้งได้สำหรับฝึกงานและตรวจ acceptance โดยใช้ Next.js, PostgreSQL migrations, owner services, authorization, version guards, idempotency และ audit ชุดเดียวกับระบบจริง ฐานข้อมูล ไฟล์ session และ credentials แยกจาก development และ production ทั้งหมด

## เริ่มใช้งาน

```bash
bun run demo:prepare
bun run demo:start -- <run-id>
bun run demo:verify
```

คำสั่งแรกสร้างฐาน PostgreSQL หกชุดและคืน `run`, ตำแหน่ง credentials และ `demo-guide.json` ซึ่งอ่านได้เฉพาะเจ้าของไฟล์ คำสั่งที่สองเปิดสำเนา ERPX ที่ `http://127.0.0.1:3102` ห้ามใช้ port 3000, 3001, 3100, 3101 หรือ 8080 คำสั่ง `demo:verify` เดินวงจรเดียวกันผ่าน owner services แบบอัตโนมัติและตรวจรับของ 6+4, ใบแจ้งหนี้ผู้ขายหนึ่งใบที่ครอบคลุม receipt ทั้งสอง, bank evidence, stock value, paid AP, balanced journals และ progress 6/6

Scenario เริ่มต้นคือใบขอซื้อ Laptop 10 เครื่อง ระบบสร้างใบขอซื้อ draft ผ่าน `saveEmployeeRequest` จริงและตั้ง Company supplier-invoice policy เป็น `required` ผ่าน owner service พร้อมบัญชี Employee, Manager, Purchasing, Warehouse, Accounting, Finance, Treasury และ Administrator คู่มือระบุลำดับบทบาท URL และผลที่ต้องเห็นตั้งแต่ submit จนถึง bank-confirmed supplier payment

คู่มือมี scenario ทางเลือกสำหรับ split sourcing หลัง reset: แบ่งรายการเดิมเป็นหลาย RFQ/หลายผู้ขาย/หลาย PO และทดลองซื้อด้วยหน่วย `each` หรือ `case` พร้อมอัตราแปลง ระบบแสดง requested, allocated และ remaining, ป้องกันการจัดสรรเกินแม้ส่งคำสั่งพร้อมกัน และคืนจำนวนเมื่อยกเลิก RFQ. Fixture เตรียมสินค้าหน่วย each/case และผู้ขายสมมติสองรายไว้ให้ใช้กับขั้นตอนนี้

คู่มือมี Finance close scenario สำหรับ January และ February 2027. Fixture ใช้ chart เดียวกับบริษัทจำลอง, มี opening journal ที่ post แล้ว, customer/supplier schedules ที่ตรงกับ AR/AP control accounts และ sandbox bank statement ณ วันสิ้นงวดทั้งสองเดือน. `Finance preparer` เริ่ม close และ `Finance approver` คนละบัญชีอนุมัติ; หน้า Close ต้องแสดง difference เป็นศูนย์ก่อนกดได้. การ destroy run จะลบฐานข้อมูลและหลักฐานของสองงวดนี้พร้อมข้อมูลจำลองส่วนอื่น โดยไม่แตะ development หรือ production data.

Scenario `finance-controlled-bank-payment` เริ่มจากใบเจ้าหนี้สมมติ THB 125 ที่ post แล้วและ exact sandbox bank row ใน March 2027: Finance preparer เตรียม payment, Finance approver คนละบัญชีอนุมัติ, Treasury คนที่สาม release แล้ว confirm. ระบบลง GL, allocate invoice, settle payment และ match bank rowใน transaction เดียว และผลยังอยู่หลัง reload. Scenario แยกเดือนจาก January/February close จึงรันต่อกันได้โดยยอดสอง close ไม่เปลี่ยน. ระบบไม่เรียกธนาคารจริง.

Scenario `traceable-receipt-and-quality-release` เตรียมสินค้า lot-tracked และ serial-tracked ไว้ใน Product master โดยยังไม่สร้างยอดสต็อก ผู้ใช้ต้องสร้างและอนุมัติ PO จริง จากนั้นคลังบันทึก lot/serial แยกแต่ละรายการและสามารถกักกัน lot ตั้งแต่รับของได้ ระบบสร้าง Quality hold อัตโนมัติ แสดง Available, Reserved, Quarantined และ In transit แยกกัน และบังคับให้ผู้ตรวจและผู้ปล่อยสินค้าเป็นคนละคนกับผู้สร้าง hold

## ตัวอย่าง Support และงานมอบหมายแทน

Scenario `temporary-task-delegation` ใน `demo-guide.json` ใช้ผู้สร้างงาน พนักงานผู้รับผิดชอบ ผู้ทำงานแทน และผู้ตรวจ completion คนละบัญชี สร้างงานจาก Work → New task แล้วมอบหมายชั่วคราวพร้อมวันหมดอายุ ผู้ทำแทนบันทึกความคืบหน้าได้ เมื่อผู้สร้างถอนสิทธิ์พร้อมเหตุผล เจ้าของงานรับช่วงต่อและส่งให้ผู้ตรวจอิสระอนุมัติ เปิด Delegation history เพื่อตรวจผู้มอบหมาย เวลา เหตุผล และการถอนหรือหมดอายุ ประวัติมีให้เฉพาะผู้ที่ยังมีสิทธิ์อ่านงานนั้น

สำหรับ Support ให้ใช้ quotation ใน scenario `sales-to-collection` เป็นเอกสารต้นทาง: `holding_owner` เปิด Support และสร้างเคสให้ `support_agent`; agent รับงาน บันทึกสถานะรอลูกค้า กลับมาทำต่อ และเสนอ resolution; `support_manager` ส่งกลับแก้ไขได้ก่อนยืนยันผล เจ้าหน้าที่ที่มี `sales_operator` สลับ role แล้วบันทึก Customer contact note หลังเสนอ resolution โดยช่องเวลารองรับวินาที ผู้จัดการอีกบัญชีเลือก note นั้นเพื่อยืนยันและปิดเคส จากนั้นทดลอง reopen และ cancel พร้อมเหตุผลได้ นี่เป็นหลักฐานที่พนักงานบันทึกแทนลูกค้า ไม่ใช่ Portal confirmation หรือการส่งอีเมลจริง

```bash
ERPX_DEMO_RUN=<run-id> ERPX_E2E_BASE_URL=http://127.0.0.1:3102 bunx playwright test tests/e2e/demo-task-delegation.spec.ts tests/e2e/demo-support-lifecycle.spec.ts --project=desktop
```

## ตัวอย่างการเปลี่ยนองค์กร

เมื่อเริ่ม Demo ใหม่ `onboard` จะทำ initial Company provisioning แบบ atomic ใน Holding ที่จองไว้: สร้าง Company, สาขาแรกและ Team, policy, staff levels 1–6, CEO position และวางบัญชีเจ้าของเป็น CEO. ระบบจะบันทึก provisioning receipt ก่อนเปิด binding เป็น active; หาก control-plane retry ภายหลังจะอ่าน receipt เดิมและไม่สร้างข้อมูลซ้ำ. หลักฐาน automated simulation อยู่ที่ `docs/testing/evidence/2026-09-22-company-provisioning.txt`.

Scenario `reviewed-organization-setup` ใช้ `holding_owner` และ `hr_admin` คนละบัญชีเพื่อตั้งสาขาและแผนกผ่าน draft, submit, return for correction และ approve/apply ในหน้า People → Organization ผู้อนุมัติทดสอบระงับสาขาที่ยังมีแผนกใช้งานอยู่ได้เพื่อดูเหตุผลที่บล็อก จากนั้นถอนคำขอและดำเนินการตามลำดับ dependency จริง ผู้อนุมัติเข้าจาก Work → Organization changes ผู้ขอเปิดคำขอที่ส่งกลับจาก Inbox → Updates ได้โดยตรง ลิงก์ระบุคำขอแน่นอนและยังใช้ได้หลังเปลี่ยนสถานะ แจ้งเตือนบันทึกพร้อม transaction และตรวจสิทธิ์ Company HR ปัจจุบันก่อนแสดง ตัวกรอง Change status และช่องค้นหาช่วยเปิดทั้งงานค้าง Applied และ Cancelled พร้อม Decision history และแบ่งหน้า 20 รายการ

การย้าย Department ใช้ `Relocate to another branch` และต้องผ่านผู้อนุมัติ Company HR อีกคน. ระบบเก็บ Department route เดิมเป็น archived history และสร้าง node ที่ Branch ใหม่พร้อมย้าย active staff, ตำแหน่ง และสิทธิ์แบบ Department ไปด้วยกัน. การ archive แสดง node ในประวัติและกด Reactivate ได้เมื่อผ่าน independent review; ถ้ายังมี dependency ใช้งาน ระบบบอกสิ่งที่ต้องย้ายหรือปิดก่อน.

```bash
ERPX_DEMO_RUN=<run-id> ERPX_E2E_BASE_URL=http://127.0.0.1:3102 bunx playwright test tests/e2e/demo-organization-lifecycle.spec.ts --project=desktop
```

หลังจัดโครงสร้างครบ ให้เปิด Settings → Business readiness เพื่อตรวจ Company setup acceptance. ผู้ขอส่งหลักฐานให้ Company administrator อีกคนตรวจและกด Accept หรือ Return for correction ได้; ผู้ขอเดิม revoke การรับรองได้เมื่อโครงสร้างเปลี่ยน. สถานะและเหตุผลอยู่ใน Work, Inbox และ immutable acceptance history. ถ้า checklist ยังไม่ครบ ระบบจะบล็อกการส่งคำขอพร้อมบอกเจ้าของและสิ่งที่ต้องแก้.

## ตรวจเมนูและบริบทบนมือถือ

บัญชี HR ใน Demo สามารถเปิด breadcrumb/Search, สลับ Branch กลับ Company และเปลี่ยนบทบาท HR ↔ Treasury จาก Account menu ได้ ชื่อบริษัทและบทบาทปัจจุบันต้องตรงกับ workspace หลัง reload บนหน้าจอเล็กหัวหน้าจอแบ่งเส้นทางหน้าเป็นแถวที่สอง กดเมนู breadcrumb เพื่อดูเส้นทางเต็มได้

```bash
ERPX_DEMO_RUN=<run-id> ERPX_E2E_BASE_URL=http://127.0.0.1:3102 bunx playwright test tests/e2e/demo-workspace-header.spec.ts --project=desktop --project=mobile
```

## Reset อย่างปลอดภัย

```bash
bun run demo:destroy -- <run-id>
# ถ้า process เพิ่งถูกหยุด ให้เรียก destroy ซ้ำหลัง process ออก
bun run demo:prepare
```

`destroy` ตรวจ run ID, process command, database prefix และ database owner ก่อนลบ ไม่มี wildcard และไม่เชื่อมต่อฐาน development/production การ Prepare ใหม่สร้างข้อมูลตั้งต้นและรหัสผ่านใหม่ ไม่ส่งอีเมล ไม่เรียกธนาคารหรือ payment provider และไม่ถือว่า provider/production acceptance ผ่าน

Static prototype ใน `Designed/mockup/` ยังคงเป็นพื้นที่ทดลองหน้าตาและ interaction แยกต่างหาก ข้อมูลทั้งหมดใน prototype อยู่ใน browser และไม่ใช่ authority record ของบริษัท Demo นี้
