# Map — รวม Docs ให้ Seamless (Next.js Era)

> Tracker: Local Markdown (`.scratch/docs-seamless/`) — default เพราะยังไม่ได้รัน `/setup-matt-pocock-skills` เพื่อเลือก GitHub Issues. ถ้าจะย้ายไป GitHub ให้รัน setup แล้ว `gh issue create --label wayfinder:map` ด้วยเนื้อหานี้

## Destination

สเปคที่ทำให้ docs ทั้งชุดเหลือชุดเดียวไม่ซ้ำไม่ย้อนแย้ง พร้อมสารบัญนำทางชัดเจน — ตัดสินใจครบแล้วพร้อมส่งต่อให้คน/เอเจนต์ไปลบ/รวมไฟล์จริงได้ทันที ไม่เหลืออะไรต้องตัดสินใจค้าง (Spec only ไม่ได้ลงมือย้ายไฟล์ใน Map นี้)

_scope:_ ทุกไฟล์ markdown ใน repo ปัจจุบันบน `development` (`29136c81` หลัง cutover Next.js `d15909bb`): `Plan/` + `Plan.legacy-2026-08-29/` + `Plan.legacy-codebase-2026-08-29/` + `adr/` (19) + `audits/` (330 โฟลเดอร์ 72M) + `docs/` (`docs/contracts/proto` 37 ไฟล์ + `docs/schema` 28 ไฟล์ + `docs/README.md`) + `Designed/` + `Delivery/` + `CONTEXT.md` + `Plan/glossary.md` + `README.md` + `AGENTS.md` + `CLAUDE.md` + `CUTOVER.md` + `CONTRACT_POLICY.md` + `fixtures/`/`ops/` + `.agents/`≈`.claude/` + `skills-lock.json` + `.gitignore` + `.github/workflows` · ประวัติ Rust ทั้งหมดอยู่บน `backup/rust-2026-09-02` tag `rust-archive-2026-09-02` + baseline `cc0887d` (อ่าน `CUTOVER.md` และ `docs/README.md`)

## Notes

- Single-context: `CONTEXT.md` อยู่ root, `adr/` อยู่ root — ไม่ใช่ multi-context (`CONTEXT-MAP.md` ไม่มี)
- ทุก session ต้องปรึกษา skills: `domain-modeling`, `grilling`, `research`, `prototype` ตามชนิดตั๋ว — `grilling` ต้องเรียก `grilling` + `domain-modeling` คู่กัน
- Wayfinder เป็น planning by default — ทำแค่ตัดสินใจ ไม่ได้ทำ deliverable; 1 ตั๋วต่อ 1 session (ยกเว้น `research` ทำขนานได้)
- มติที่ล็อกแล้ว: `audits/` + `Plan.legacy*` ให้ **prune** (ตัดทิ้ง) ไม่ได้เก็บดอง verbatim; ticket ประเภท `grilling` ต้องคุยกับคนจริง (HITL) — เอเจนต์ห้ามตอบแทน
- ภาษาหลักของ repo คืออังกฤษ แต่ Map และตั๋วนี้เขียนไทยตามคำขอ เพื่อให้ทีมไทยอ่านรู้เรื่อง — ศัพท์บัญญัติใน `CONTEXT.md` ยังเป็นอังกฤษ

## Decisions so far

- [01 — สำรวจ + ตารางความซ้ำ/ย้อนแย้ง](issues/01-inventory-contradiction-matrix.md) — research AFK เสร็จบน `research/inventory-contradiction-matrix:f4727aae` → `docs/research-01-matrix.md` — พบ 28 แถวซ้ำ/ย้อนแย้ง (critical 7 จาก second cutover missing) + 558 dead links `crates/proto` + `.agents`≡`.claude` + audits 72M dup; ปลดล็อก frontier 02,03,04,05
- [09 — Designed + Skills ซ้ำซ้อน](issues/09-designed-skills-dedup.md) — research AFK เสร็จบน `research/designed-skills-dedup:707ac4eb` → `docs/research-09-dedup.md` — `.pen` 15M/894 frames (272 Page) เป็น single source ไม่ซ้ำ Plan, `.agents`≡`.claude` 100% ต่างแค่ 5× `.barlevalon-installed` — แนะนำลบ `.claude/skills` เหลือ `.agents/skills` เดียว + เพิ่ม `Designed/README.md` stub

## Not yet specified

<!-- หมอก — โซนที่ยังคมไม่พอเป็นตั๋ว จะแตกเป็นตั๋วใหม่เมื่อ frontier คืบ -->

- หลัง prune `audits/` แล้วสารบัญประวัติจะลึกแค่ไหน ลิงก์ย้อนกี่ gate / เก็บ `audits/CURRENT-CERTIFICATION.md` อย่างเดียวพอไหม
- `ops/runbooks/` + `fixtures/` จะแก้อ้างอิง `cargo xtask` / SQLx แค่ไหน หรือติดป้าย Deprecated แล้วชี้ไป docs ใหม่
- สัญญา `@<context>/<id>` + `/reference/item/:id` + `@-mention` ใน `Plan/01-vision-and-scope.md` จะเก็บไว้หรือเลิก — เกี่ยวกับว่าจะมี field-level chip ต่อไหม
- นโยบาย ไทย/อังกฤษ ใน docs (Plan บอก UI ไทย+อังกฤษ แต่ docs เป็นอังกฤษ) จะกำหนดอย่างไรในชุด seamless
- ลบแล้วต้องทิ้ง stub ชี้ไป `rust-archive-2026-09-02` ใน `docs/README.md` หรือลบเกลี้ยง
- ขนาด `audits/` ที่ prune แล้วจะรายงานอย่างไรไม่ให้สับสนกับ `docs/` ที่เป็น living spec

## Out of scope

<!-- ไม่ทำใน Map นี้ — กลับมาเฉพาะเมื่อวาดปลายทางใหม่ -->

- เขียนโค้ด Next.js, ออกแบบ DB/Auth/routing, เลือก Prisma/Drizzle — ยกเว้นที่สะท้อนใน docs
- เขียนเนื้อหา `Plan/02-capabilities/**` 31 contexts ใหม่ทั้งชุด — แค่จัดที่วาง/ตัดซ้ำ ไม่แก้สาระธุรกิจ
- ปั่น evidence `audits/` ใหม่ — แค่ตัด/ทำดัชนีของเก่า
- สร้าง `Plan/tools/validate_requirements.py` หรือ CI ใหม่ — อยู่นอกปลายทางนี้
- แก้ไข `Designed/MainDesign.pen` ตัว canvas เอง — แค่จัดวิธีอ้างอิงใน docs
