Status: open
Type: grilling
Blocked by: 01

# 02 — คำศัพท์อยู่ที่เดียวที่ไหน

## Question

ตัดสินใจว่า **คำศัพท์บัญญัติ (Ubiquitous Language) จะอยู่บ้านเดียวที่ไหน** — ต้องเลือกให้เหลือที่เดียว ไม่ให้มีสองบ้านทับกัน แล้วกำหนดวิธีเลิกใช้อีกที่

ตัวเลือกที่ต้องเทียบ (เรียก `skill("grilling")` + `skill("domain-modeling")` คู่กัน):
- `CONTEXT.md` (root, 46 บรรทัด, บอกว่า code-derived, implementation-free) — ปัจจุบันเป็น glossary เดียวที่ถูกอ้างถึง
- `Plan/glossary.md` (แค่ pointer ไป CONTEXT.md — ควรคงเป็น pointer หรือลบให้เหลือที่เดียว)
- `Plan.legacy-2026-08-29/glossary.md` + `Plan.legacy-codebase-2026-08-29/glossary.md` + `Plan.legacy-codebase-2026-08-29/contexts/*.md` (คำศัพท์เก่าที่ยังทับ)

หัวข้อตัดสินใจ:
- จะใช้ `CONTEXT.md` เป็น single source จริงไหม? `Plan/glossary.md` จะเหลือเป็น redirect 1 บรรทัดหรือลบไปเลย?
- Legacy glossaries จะ prune หรือเหลือ stub ชี้ไป `CONTEXT.md` + ระบุว่า archive?
- กฎอัปเดตคำศัพท์ต่อจากนี้: แก้ที่ไหน ใคร approve ต้องมี ADR ไหม? เขียนไว้ใน `AGENTS.md` ส่วนไหน?

HITL — ต้องคุยกับคนที่พูดแทนโดเมนได้ เอเจนต์ห้ามตอบแทนฝั่งคน

## Answer

<!-- หลังคุย: สรุปคำตอบ + ก่อน/หลัง tree + ลิงก์ branch ถ้ามี -->

## Comments
