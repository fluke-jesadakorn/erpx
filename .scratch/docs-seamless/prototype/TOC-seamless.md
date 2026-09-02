# TOC — Seamless Docs (Prototype throwaway)

> Prototype for ticket 08 — ไม่ใช่ของจริง แค่ให้ดูแล้วบอกว่า "อันนี้แหละ" หรือ "แก้ตรงนี้" ก่อนเอาไปทำจริงบน `src/app/docs` หรือ `docs/TOC.md`

## ต้นไม้ 3 ชั้น (หลัง prune)

```
README.md (entry — Next.js reboot 2026-09-02, recovery 3 ทาง: backup/rust-2026-09-02, tag rust-archive-2026-09-02, cc0887d)
├─ AGENTS.md + CLAUDE.md (@AGENTS.md) — nextjs-rules + barlevalon skills (ห้ามแก้ใน fences)
├─ CUTOVER.md — 2 entries: 2026-09-02 reverse (ใหม่, บนสุด) + 2026-07-31 Folio→Rust (เดิม)
├─ Plan/ — living spec (31 contexts)
│  ├─ README.md — Code is truth → docs/contracts/proto (archive) + src/app (living)
│  ├─ 01-vision-and-scope.md — business vision (แก้บรรทัด Rust→Next.js)
│  ├─ 02-capabilities/ 15 กลุ่ม/31 contexts — คง business, banner impl moved to src/app + ADR-020
│  ├─ 03-cross-cutting/@-and-ai/ 33 ไฟล์ — คงไว้, ไม่ซ้ำ 02 (deep spec)
│  ├─ traceability.md — Archived crates/* → docs/contracts/proto (archive) + src/app (living) next/
│  ├─ glossary.md — pointer 5 บรรทัด → CONTEXT.md (canonical)
│  └─ LEGACY.md — stub 10 บรรทัด ชี้ backup (แทน 2 โฟลเดอร์ legacy ที่ลบ)
├─ CONTEXT.md — ศัพท์บัญญัติเดียว (46 terms, implementation-free)
├─ adr/ — 19 ฉบับ: Reconfirmed 13 ✅ + Deprecated 3 (010,014,016) + Superseded 002 → ADR-020 Next.js stack (ใหม่)
├─ docs/ — archive + living แยกชัด
│  ├─ README.md — banner Reference-only (37 proto + 28 schema จาก f97b74d6) + ลิงก์ backup
│  ├─ contracts/proto/ — 37 proto archive (ห้ามแก้)
│  ├─ contracts/descriptor/ — sha256/bin
│  ├─ contracts/next/ — (อนาคต) openapi.yaml / route table หลังมี src/app จริง
│  ├─ schema/migrations* — 28 SQL archive (ห้ามแก้) — Prisma/Drizzle อยู่ข้างนอก
│  └─ TOC.md — (ตัวนี้) สารบัญเดียวที่ลิงก์ไปทุกที่
├─ Designed/ — Pencil canvas เดียว
│  ├─ AGENTS.md — Pencil-only guardrail (canonical)
│  ├─ MainDesign.pen 15M + images/ 27 PNG 16M — single source route /work-queue, /billing/* (ห้าม grep)
│  └─ README.md — stub ลิสต์ 27 รูป + ลิงก์ไป Plan spine (ใหม่)
├─ audits/ — หลัง prune (72M → <20M)
│  ├─ README.md — สารบัญเดียว (ใหม่) ลิงก์ไป ~40 โฟลเดอร์ที่เหลือ
│  ├─ CURRENT-CERTIFICATION.md — ติดป้าย Dioxus cert + ลิงก์ไป backup
│  ├─ DIOXUS-CUTOVER*, ADR-017-PHASES, e2e-all-modules, finance_live_full, finance_darkmode_fixed — เหลือ
│  └─ (ลบ) G0..G11 เก่า, finance_darkmode, finance_images ซ้ำ, *.log/.DS_Store
├─ fixtures/ + ops/ — runbooks ใหม่ (cargo xtask → npm run) + deterministic fixtures (คงเป็น reference)
├─ src/app/ — living Next.js routes (page.tsx, layout.tsx, globals.css) — จะเพิ่ม route ตาม Plan
├─ .agents/skills/ — canonical 6 skills (ลบ .claude/skills เหลือ symlink หรือลบ)
├─ .github/workflows/ — nextjs.yml (lint+build) แทน erpx-targets.yml (ติด Deprecated)
└─ .gitignore — Next.js + legacy Rust ignore รวม

## วิธีเดิน docs ไม่หลง (3 เส้น)

1. **คนใหม่เริ่มจาก README.md** → Plan/01 → Plan/02 context ที่สนใจ → CONTEXT (ศัพท์) → src/app (โค้ดจริง) → ถ้าอยากรู้ประวัติ → CUTOVER + docs/archive + backup branch
2. **หาศัพท์:** ทุกที่ลิงก์ไป CONTEXT.md — ไม่มีศัพท์ 2 บ้าน
3. **หาประวัติ:** CUTOVER 2 entries + Plan/LEGACY.md + docs/README → backup/rust-2026-09-02

## สิ่งที่ prototype นี้พิสูจน์

- ไม่มีไฟล์ไหนทับกัน 2 ที่อีก (legacy ลบ, audits prune, contracts แยก archive/living, skills เหลือที่เดียว)
- ไม่มี contradictions ที่ critical 7 เหลือ (CUTOVER 2 entries, CONTRACT_POLICY แก้, traceability ชี้ archive, Delivery ยังไม่ได้อยู่ใน tree นี้ — จะ archive แยก)
- สารบัญเดียว (`docs/TOC.md` หรือ `src/app/docs`) พอ

## Next

ถ้า prototype นี้โอเค — เอา tree นี้ไปทำจริง: สร้าง `docs/TOC.md` + `Plan/LEGACY.md` + `Designed/README.md` + `audits/README.md` + แก้ `CUTOVER.md` + `CONTRACT_POLICY.md` + ลบ legacy/audits ตามเกณฑ์ แล้ว hand-off
