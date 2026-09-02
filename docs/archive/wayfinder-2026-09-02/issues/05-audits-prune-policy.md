Status: resolved
Type: grilling
Blocked by: 01
Claimed-by: muse-spark-1.2
Resolved-at: 2026-09-02

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

**ตัดสินใจ (grilling — prune ตามมติ):**

**หลัก prune (4 ข้อ):**
1. **ลบซ้ำ 100%:** `finance_darkmode/` (2.7M) เก็บ `finance_darkmode_fixed/` (2.7M) ชุดเดียว, `finance_images/` 852K รวมเข้า `finance_darkmode_fixed` หรือลบ, `finance_live` / `finance_live_auth` / `finance_live_real` เหลือ `finance_live_full` (ชุดเดียว 10 รูปครบ)
2. **ลบ log/ignore:** `audits/finance-e2e-2026-08-30.log` 431K, `audits/web-e2e/.DS_Store`, `*.log` ทั้งหมด (ตรวจด้วย `git check-ignore` → `.gitignore:26`)
3. **ลบ evidence เก่าที่ซ้ำ gate:** `G0-*` .. `G11-*` ที่เก่ากว่า `CURRENT-CERTIFICATION.md:20` (2026-08-19) และไม่ใช่ `DIOXUS-CUTOVER` / `ADR-017-PHASES` / `e2e-all-modules` — ลบ ~200 โฟลเดอร์
4. **เหลือ:** `audits/CURRENT-CERTIFICATION.md` (ต้องติดป้ายว่า Dioxus cert — ดู 07), `audits/DIOXUS-CUTOVER*`, `audits/ADR-017-PHASES-0-6*`, `audits/e2e-all-modules/README`, `audits/finance_live_full` (10 รูป), `audits/finance_darkmode_fixed` (20 รูป) — ประมาณ 30-40 โฟลเดอร์จาก 330

**รูปแบบหลัง prune:** สร้าง `audits/README.md` สารบัญเดียว ลิงก์ไปโฟลเดอร์ที่เหลือ + บอกวันที่ + ขนาดก่อน/หลัง (72M → เป้าหมาย <20M) — ไม่ปล่อยโครงโฟลเดอร์เปล่า

**รายงานขนาด:** หลัง prune `du -sh audits` ต้อง <20M และ `ls audits | wc -l` ~40 — เขียนใน `audits/README.md` + `docs/README.md` recovery note (hash ที่ลบไม่ต้องเก็บ แค่บอกว่า prune ตาม policy นี้ + ชี้ไป backup tag `rust-archive-2026-09-02` ที่มีครบ)

*Grilling ถามว่า evidence ไหนห้ามลบ — คำตอบ: เหลือเฉพาะที่เกี่ยวกับ Next.js era + certification ล่าสุด*

## Comments

- Resolved: 2026-09-02 — กรอบ prune ชัดแล้ว; ปลดล็อก 08 บางส่วน
- ขนาดเป้าหมาย <20M เพื่อให้ docs เบาแต่ตรวจย้อนหลังได้
