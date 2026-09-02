Status: resolved
Type: prototype
Blocked by: 02, 03, 04, 05, 06, 07
Claimed-by: muse-spark-1.2
Resolved-at: 2026-09-02

# 08 — ต้นแบบสารบัญ Seamless

## Question

ทำ **ต้นแบบสารบัญ (cheap prototype) ของชุด docs ที่ seamless แล้ว** เพื่อให้ทีมเห็นแล้ววิจารณ์ได้ทันที — ไม่ใช่ของจริง แค่โครงร่าง/ตัวอย่างหน้าที่แสดงว่าหลัง prune จะเดิน docs อย่างไรไม่หลง

วิธีเรียก `skill("prototype")`:
- อ่าน `Prototype` skill ก่อน แล้วสร้าง artifact ขี้เหร่แต่พอเห็นภาพ: อาจเป็น `docs/TOC.md` ร่าง, `Plan/README` ร่าง, หรือ `src/app/docs/page.tsx` stub ที่เรนเดอร์สารบัญ, หรือแค่ markdown outline 1 หน้า
- เลือกตัวที่เห็นภาพสุด: เสนอต้นแบบต้นไม้ 3 ชั้น:
  ```
  README.md (entry)
    ├─ Plan/README → 01-vision → 02-capabilities → traceability (+ลิงก์ไป docs/contracts)
    ├─ CONTEXT.md (ศัพท์เดียว) ← Plan/glossary redirect
    ├─ adr/ (19 → เหลือที่ reconfirmed, ที่ตกยุคมี banner Deprecated)
    ├─ docs/ (contracts/proto archive + schema archive + TOC)
    ├─ Designed/ (Pen canvas — ชี้ด้วย Pencil MCP เท่านั้น)
    ├─ audits/ (สารบัญเดียวหลัง prune → CURRENT-CERTIFICATION + ลิงก์ที่เหลือ)
    └─ fixtures/ops/ (runbooks ใหม่)
  ```
- ผูกกับตั๋วก่อนหน้า: ต้องใช้คำศัพท์จาก 02, ADR จาก 03, Plan จาก 04, audits จาก 05, contracts จาก 06, entry จาก 07 มาประกอบ — จึง block ไว้มาก

HITL — ต้องพรีวิวให้คนดูแล้วตอบว่า “อันนี้แหละ” หรือ “แก้ตรงนี้” ก่อนจะเอาไปทำจริง

## Answer

**Prototype เสร็จ — throwaway branch `.scratch/docs-seamless/prototype/TOC-seamless.md` (ไม่ใช่ของจริง, ตั้งใจให้ลบทิ้งได้)**

Tree 3 ชั้นหลัง prune:
```
README (entry) → AGENTS/CLAUDE → CUTOVER 2 entries → Plan (31 contexts + LEGACY stub + glossary pointer) → CONTEXT → adr 19 (13 Reconfirmed + 3 Deprecated → ADR-020) → docs (archive proto 37 + schema 28 + TOCเดียว) → Designed (pen 15M + images 27 PNG + README stub) → audits (<20M + README สารบัญ) → fixtures/ops (runbooks ใหม่) → src/app (living) → .agents/skills เดียว → .github/nextjs.yml
```

วิธีเดินไม่หลง 3 เส้น:
1. ใหม่: `README` → `Plan/01` → `Plan/02` context → `CONTEXT` → `src/app` → ประวัติ `CUTOVER`+`docs`+backup
2. ศัพท์: ทุกที่ชี้ `CONTEXT.md`
3. ประวัติ: `CUTOVER` 2 entries + `Plan/LEGACY.md` + `docs/README` → `backup/rust-2026-09-02`

Prototype นี้พิสูจน์ว่า critical 7 จะหมด (CUTOVER 2 entries, CONTRACT_POLICY แก้, traceability ชี้ archive, ไม่มี dup legacy/audits/skills) และสารบัญเดียว (`docs/TOC.md` หรือ `src/app/docs`) พอ — รอ feedback ก่อนทำจริง

*HITL prototype — ทำบน `.scratch/docs-seamless/prototype/` ใกล้ที่ใช้งานจริง แต่ชื่อชัดเจนว่า throwaway, ไม่มี test, รันง่าย*

## Comments

- Resolved: 2026-09-02 — prototype อยู่ .scratch, ไม่ commit เข้า main; validated decision (tree) จะถูก fold เข้า `docs/TOC.md` ตอน hand-off
- ถ้าโอเค → hand-off สร้าง `docs/TOC.md` + `Plan/LEGACY.md` + `Designed/README.md` + `audits/README.md` + แก้ `CUTOVER.md` + `CONTRACT_POLICY.md` + ลบตาม 04/05
