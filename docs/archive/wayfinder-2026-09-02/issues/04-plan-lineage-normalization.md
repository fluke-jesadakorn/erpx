Status: resolved
Type: grilling
Blocked by: 01
Claimed-by: muse-spark-1.2
Resolved-at: 2026-09-02

# 04 — Plan จะเก็บ lineage ยังไง

## Question

ตัดสินใจวิธีจัดระเบียบ **Plan สามยุคที่ซ้อนกัน**: `Plan/` (ปัจจุบัน 6 ไฟล์ + 02-capabilities 15 กลุ่ม + traceability), `Plan.legacy-2026-08-29/` (16 ไฟล์ FSD แบบ aspirational), `Plan.legacy-codebase-2026-08-29/` (12 ไฟล์ snapshot เทคนิค) — ตอนนี้ผู้อ่านหลงระหว่างสามที่

ทางเลือก (มติที่ล็อกแล้วคือ prune — ต้องเลือกว่าจะ prune แค่ไหน):
- A) ลบ legacy ทั้งสองโฟลเดอร์ทิ้งเลย เหลือ stub `Plan/legacy-README.md` ชี้ไป `backup/rust-2026-09-02` + `cc0887d`
- B) เหลือโครงโฟลเดอร์แต่ใส่แบนเนอร์ Deprecated + ลิงก์ไปปัจจุบัน ไม่แก้เนื้อหา
- C) รวมบางไฟล์ที่ยังมีค่ากลับเข้า `Plan/` (เช่น 01-vision) แล้วลบที่เหลือ

หัวข้อตัดสินใจเพิ่มเติม:
- `Plan/README.md` ที่บอกว่า “Archived baselines: Plan.legacy-...” จะแก้เป็นอะไร
- `Plan/traceability.md` ที่ยังชี้ `crates/proto`/`crates/contexts` จะแก้เป็น `docs/contracts/proto` + Next.js route อย่างไร
- `Plan/02-capabilities/**` จะคง 31 contexts หรือต้องติดป้ายว่า business ยังเดิม แต่ impl เปลี่ยนเป็น Next.js

เรียก `grilling` + `domain-modeling` — ต้องคุยกับคนว่า legacy มีค่าพอให้เก็บเป็น archive ไหม

## Answer

**ตัดสินใจ: เลือก A — ลบ legacy ทั้งสองโฟลเดอร์ เหลือ stub เดียว (prune เต็มตามมติ)**

- **A) ลบ `Plan.legacy-2026-08-29/` (43 md, 16 ไฟล์ FSD) + `Plan.legacy-codebase-2026-08-29/` (40 md, 12 ไฟล์ snapshot) ทิ้งทั้งโฟลเดอร์** — เหตุผล: ทั้งสองคือ aspirational/technical snapshot ที่ไม่เคยเป็น living spec บน `development` ปัจจุบัน (คุณเลือก prune แล้ว) เก็บไว้ทำให้คนสับสนระหว่าง 3 ที่; ประวัติอยู่บน `backup/rust-2026-09-02` + `cc0887d` + `Plan/legacy-README.md` จะลิงก์ไป
- เหลือ **stub `Plan/LEGACY.md` (หรือ `Plan/legacy-README.md`) 10 บรรทัด**: บอกว่า “Archived baselines moved to backup/rust-2026-09-02 — see docs/README.md recovery + git log f97b74d6” — ไม่ต้องคงโครงโฟลเดอร์ (B ไม่เอา), ไม่ต้องรวมไฟล์กลับเข้า Plan (C ไม่เอา — vision ใน 01 ปัจจุบันคือ business vision ไม่ใช่ FSD)
- **`Plan/README.md:5` แก้:** จาก “Code is truth. Every capability traces to `crates/proto`…” → “Code is truth. Every capability traces to `docs/contracts/proto` (archive, 37 proto) + `src/app` (living Next.js routes). See traceability.”
- **`Plan/traceability.md` แก้:** 558 แถวที่ชี้ `crates/proto`/`crates/contexts` จะติดป้าย `Archived — see docs/contracts/proto (reference-only)` แล้วเพิ่มคอลัมน์ `Next.js route: src/app/<module>` ที่ตอนนี้ยัง placeholder; ไม่ลบคอลัมน์เก่า แต่ใส่ banner แดงว่า stale
- **`Plan/02-capabilities/**` 31 contexts คงไว้** — business 31 contexts ยังถูก (ไม่ใช่ impl) แต่ติด banner หัวไฟล์ว่า “Business scope still valid; impl moved from crates/* to src/app/* — see ADR-020”

*Grilling ถาม 3 ทางเลือก A/B/C — แนะนำ A เพราะสอดคล้อง prune*

## Comments

- Resolved: 2026-09-02 — เลือก A prune เต็ม; ปลดล็อกส่วนหนึ่งของ 07; ไม่แก้ไฟล์จริงใน Map นี้
- ตัวอย่างไฟล์ลบ: `Plan.legacy-2026-08-29/glossary.md`, `Plan.legacy-codebase-2026-08-29/contexts/finance.md` ฯลฯ (43+40 md)
