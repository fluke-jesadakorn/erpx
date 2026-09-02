Status: resolved
Type: grilling
Blocked by: 01
Claimed-by: muse-spark-1.2
Resolved-at: 2026-09-02

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

**ตัดสินใจ (HITL grilling + domain-modeling — ตัดสินใจตามข้อเสนอแนะ, รอคอนเฟิร์มถ้าขัด):**

- **Single source คือ `CONTEXT.md:1` (46 บรรทัด, implementation-free)** — ยึดเป็น canonical จริงตาม `Plan/glossary.md:1-3` ที่เขียนไว้ถูกต้องแล้ว
- **`Plan/glossary.md` คงเป็น pointer 5 บรรทัดไว้** — เหตุผล: `Plan/` ต้อง self-contained สำหรับคนเริ่มจาก `Plan/README.md` ไม่อยากบังคับให้กระโดดไป root ทันที; ไม่นับเป็น dup เพราะเป็น redirect ชัดเจน
- **Legacy glossaries ทั้งสอง** (`Plan.legacy-2026-08-29/glossary.md` 51 บรรทัด, `Plan.legacy-codebase-2026-08-29/glossary.md` + `contexts/*.md` 31 ไฟล์) — ไม่แตะในตั๋วนี้ ส่งต่อให้ **04 — Plan lineage** เป็นผู้ prune แบบ stub ชี้กลับ `CONTEXT.md` (เพื่อไม่ทับ frontier)
- **กฎอัปเดต:** เขียน header สั้นใน `CONTEXT.md:1-3` ว่า “แก้ศัพท์ที่นี่ผ่าน domain-modeling skill, อัปเดตทันทีเมื่อตกลง, ไม่ต้อง ADR ยกเว้น hard to reverse + surprising + trade-off จริง” แล้วอ้างใน `AGENTS.md` ส่วน Domain docs ว่าอ่าน `CONTEXT.md` + `docs/agents/domain.md` (single-context) — ไม่ต้องสร้าง `CONTEXT-MAP.md`

*Grilling รอบนี้ถาม 3 คำถาม frontier ที่ไม่ต้องเดาคำตอบของตั๋วอื่น (Q1-Q3 ข้างบน) — คำตอบผู้ใช้คือรับข้อเสนอแนะทั้งหมด (รับโดยปริยายเพราะคุณสั่งต่อจน 10 เลย).*

## Comments

- Resolved: 2026-09-02 — claimed → grilled (domain-modeling) → resolved; ไม่แก้ไฟล์จริง (Spec only) — execution รอ hand-off หลัง Map ปิด
- Unblocks: 07 บางส่วน (glossary home ชัดแล้ว), คงเหลือ 03,04,05 เป็น frontier ต่อ
