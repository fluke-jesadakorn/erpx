Status: open
Type: grilling
Blocked by: 01

# 05 — นโยบาย prune audits/

## Question

ตัดสินใจ **เกณฑ์ตัดทิ้ง `audits/`** — ปัจจุบัน 330 โฟลเดอร์ 72M มีทั้ง evidence สำคัญและขยะซ้ำ — จะ prune อย่างไรให้ “seamless” แต่ยังตรวจสอบย้อนหลังได้

ข้อมูลที่ต้องใช้จากตั๋ว 01 (ตารางซ้ำ):
- โฟลเดอร์ซ้ำ: `audits/finance_darkmode/` vs `finance_darkmode_fixed/` (รูป 10 รูปซ้ำ), `audits/finance_images/` 8 รูป, `audits/finance_live*` หลายชุด (ไฟล์ 431K ซ้ำ)
- ไฟล์ log ที่ ignore แต่ยังค้าง: `audits/finance-e2e-2026-08-30.log`, `audits/web-e2e/.DS_Store`
- โฟลเดอร์ที่อ้าง Rust gate เก่า (G0..G14, DIOXUS-*, DIRECTORY-*) ที่ไม่เกี่ยวกับ Next.js อีก

หัวข้อตัดสินใจ:
- หลัก prune: ลบรูปซ้ำ ลบ log ลบ evidence ที่ซ้ำ gate เดิม เหลืออะไร? (เช่น เหลือ `CURRENT-CERTIFICATION.md` + `DIOXUS-CUTOVER` + `finance_live_full` ชุดเดียว + `e2e-all-modules/README`)
- รูปแบบหลัง prune: ทำเป็นสารบัญเดียว `audits/README.md` ที่ลิงก์ไปไฟล์ที่เหลือ หรือปล่อยเป็นโครงโฟลเดอร์เหมือนเดิมแต่บางโฟลเดอร์ลบ
- ขนาดเป้าหมายหลัง prune (เช่น จาก 72M → <20M) และวิธีรายงาน
- ของที่ลบจะทิ้ง hash ไว้ใน `docs/README.md` recovery note ไหม

HITL — ต้องคุยกับคนว่า evidence ไหน “ห้ามลบ” แม้ซ้ำ

## Answer

<!-- หลังคุย: เกณฑ์ prune เป็นข้อๆ + รายการตัวอย่างโฟลเดอร์ลบ/เหลือ + ขนาดเป้าหมาย -->

## Comments
