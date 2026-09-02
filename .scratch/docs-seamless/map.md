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
- [02 — คำศัพท์อยู่ที่เดียวที่ไหน](issues/02-canonical-glossary-home.md) — ยึด `CONTEXT.md` เป็น canonical เดียว, `Plan/glossary.md` คง pointer 5 บรรทัด, legacy glossaries ส่งให้ 04 prune แบบ stub — กฎอัปเดตผ่าน domain-modeling ไม่ต้อง ADR ยกเว้น hard to reverse
- [03 — ADR ไหนยังศักดิ์สิทธิ์ในยุค Next.js](issues/03-adr-authority-nextjs-era.md) — ตาราง 19: Reconfirmed 13 (001,003-009,011-013,015,017-019), Deprecated 3 (010,014,016 Dioxus/CQRS) + 002 Superseded → ให้ ADR-020 Next.js stack ใหม่เป็น successor
- [04 — Plan จะเก็บ lineage ยังไง](issues/04-plan-lineage-normalization.md) — เลือก A prune: ลบ `Plan.legacy-2026-08-29/` (43 md) + `Plan.legacy-codebase-*` (40 md) เหลือ `Plan/LEGACY.md` stub ชี้ backup; แก้ `Plan/README.md` + `traceability.md` ให้ชี้ `docs/contracts/proto` + `src/app`
- [05 — นโยบาย prune audits/](issues/05-audits-prune-policy.md) — เกณฑ์ prune 4 ข้อ: ลบซ้ำ `finance_darkmode` เหลือ `_fixed`, ลบ `*.log/.DS_Store`, ลบ G0..G11 เก่าซ้ำ (~200 โฟลเดอร์) เหลือ ~40 โฟลเดอร์ + `CURRENT-CERTIFICATION` + `audits/README.md` สารบัญ — 72M → <20M
- [06 — docs/contracts + docs/schema เอาไว้ทำอะไร](issues/06-docs-contracts-schema-role.md) — เลือก A archive-only ตอนนี้ (reference-only + banner) + เตรียม C (`docs/contracts/next/` ในอนาคต); แก้ `CONTRACT_POLICY.md` เป็น REST/tRPC + Prisma, `traceability.md` ชี้ `docs/contracts/proto` (archive) + `src/app` (living)
- [07 — หน้าแรกนำทางให้ตรงกัน](issues/07-top-level-entry-coherence.md) — เพิ่ม `CUTOVER.md` entry 2026-09-02 reverse (f97b74d6+tag+cc0887d), คง `CLAUDE.md` เป็น `@AGENTS.md`, ล็อก `AGENTS.md`/`README.md` เป็น Next.js แล้ว, แก้ `CONTRACT_POLICY.md` ตาม 06, `.env.example` เป็น NEXT_PUBLIC_*, workflows ติด Deprecated + เตรียม Next.js CI (ticket 10)
- [08 — ต้นแบบสารบัญ Seamless](issues/08-seamless-navigation-prototype.md) — prototype `.scratch/docs-seamless/prototype/TOC-seamless.md` tree 3 ชั้น (README→Plan→CONTEXT→adr→docs→Designed→audits→src/app) + 3 เส้นเดินไม่หลง — validated, รอ hand-off เป็น `docs/TOC.md`
- [10 — Next.js CI + Traceability แทนของเก่า](issues/10-nextjs-ci-traceability.md) — task AFK spec: สร้าง `.github/workflows/nextjs.yml` (lint+build) + ติดป้าย `erpx-targets.yml` Deprecated, แก้ `Plan/traceability.md` ชี้ `docs/contracts/proto` (archive) + `src/app` (living) + ป้าย Archived, `ops/runbooks` ติด Deprecated

## Not yet specified

<!-- หมอก — โซนที่ยังคมไม่พอเป็นตั๋ว จะแตกเป็นตั๋วใหม่เมื่อ frontier คืบ — อัปเดตหลังปิด 02-10: หลายจุดคมแล้วและแตกเป็นตั๋ว/ตัดสินใจแล้ว -->

- ~~หลัง prune `audits/` สารบัญลึกแค่ไหน~~ → ตัดสินใจแล้วใน 05: เหลือ ~40 โฟลเดอร์ + `audits/README.md` สารบัญเดียว (CURRENT-CERTIFICATION + ที่เหลือ), 72M→<20M
- ~~`ops/runbooks/` จะแก้ cargo อย่างไร~~ → ตัดสินใจแล้วใน 10: ติด banner Deprecated — Dioxus-era, ไม่ลบ, `fixtures/` คงเป็น reference
- ~~ลบทิ้ง stub หรือไม่~~ → ตัดสินใจแล้วใน 04+06: เหลือ `Plan/LEGACY.md` + `docs/README` banner ชี้ backup, ไม่ลบเกลี้ยง
- ~~ขนาด prune รายงานอย่างไร~~ → ตัดสินใจแล้วใน 05: เขียน `audits/README.md` บอกก่อน/หลัง + ชี้ backup
- สัญญา `@<context>/<id>` + `/reference/item/:id` + `@-mention` ใน `Plan/01` จะเก็บหรือเลิก — ยังไม่คมพอ (impl รอ Next.js field chip จริง) — อาจต้องแตกเป็นตั๋วใหม่หลัง hand-off ถ้า Next.js จะมี field-level chip
- นโยบาย ไทย/อังกฤษ ใน docs — ยังไม่คมพอ (Plan บอก UI ไทย+อังกฤษ แต่ docs เป็นอังกฤษ) — เก็บเป็นหมอกต่อ ไม่ block hand-off

## Out of scope

<!-- ไม่ทำใน Map นี้ — กลับมาเฉพาะเมื่อวาดปลายทางใหม่ -->

- เขียนโค้ด Next.js, ออกแบบ DB/Auth/routing, เลือก Prisma/Drizzle — ยกเว้นที่สะท้อนใน docs
- เขียนเนื้อหา `Plan/02-capabilities/**` 31 contexts ใหม่ทั้งชุด — แค่จัดที่วาง/ตัดซ้ำ ไม่แก้สาระธุรกิจ
- ปั่น evidence `audits/` ใหม่ — แค่ตัด/ทำดัชนีของเก่า
- สร้าง `Plan/tools/validate_requirements.py` หรือ CI ใหม่ — อยู่นอกปลายทางนี้
- แก้ไข `Designed/MainDesign.pen` ตัว canvas เอง — แค่จัดวิธีอ้างอิงใน docs
