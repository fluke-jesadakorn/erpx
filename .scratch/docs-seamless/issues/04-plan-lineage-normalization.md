Status: open
Type: grilling
Blocked by: 01

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

<!-- หลังคุย: เลือก A/B/C + รายการไฟล์ที่ลบ/เหลือ + ข้อความ stub -->

## Comments
