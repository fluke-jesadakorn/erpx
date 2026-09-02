Status: resolved
Type: task
Blocked by: 03, 04, 06
Claimed-by: muse-spark-1.2
Resolved-at: 2026-09-02

# 10 — Next.js CI + Traceability แทนของเก่า

## Question

ทำงาน (task) ที่ต้องทำก่อนตัดสินใจสุดท้ายจะสมบูรณ์ — **สร้าง CI กับ traceability ที่ใช้ได้จริงบน Next.js** แทนของเก่าที่ stale (ticket 07 พูดถึงแต่ยังไม่ได้ทำ, หมอก Not yet specified บางส่วนเพิ่งคมพอ)

สิ่งที่ต้องทำ (AFK ได้ — ไม่ต้องคุยกับคน แต่ต้องทำให้เสร็จจึงจะ hand-off ได้):
- **CI:** สร้าง `.github/workflows/nextjs.yml` ที่ trigger `src/**`, `Plan/**`, `adr/**` รัน `npm run lint && npm run build` (แทน `erpx-targets.yml` ที่ trigger `apps/**,crates/**` แล้วรัน `cargo xtask verify-core/verify-sqlx` — critical I-1) — ติดป้าย `erpx-targets.yml` ว่า Deprecated — Rust CI, see backup/rust-2026-09-02
- **Traceability:** แก้ `Plan/traceability.md` (567 แถว 558 dead links `crates/proto`) ให้ชี้ `docs/contracts/proto` (archive) + `src/app` (living) + เติม column `Next.js route` / `status: archived|living` — หรือถ้ายังไม่มี route จริง ให้ใส่ stub `TBD — see docs/contracts/next/` (ตาม 04+06)
- **Ops/Fixtures หมอกที่ค้าง:** ตัดสินใจจาก 3 หมอกที่ยังไม่เป็นตั๋ว — `ops/runbooks/` จะแปะ banner `Deprecated — Dioxus-era, cargo xtask no longer exists` หรือลบ, `fixtures/` จะคงเป็น reference (ไม่ต้องแก้), สัญญา `@-mention` จะคงไว้ตาม `Plan/01` แต่ impl รอ Next.js

งานนี้ไม่ตัดสินใจใหม่ แค่ทำ checklist ให้เสร็จ — result จะเป็นไฟล์ใหม่ 2 ไฟล์ + ป้าย Deprecated 1 ไฟล์ — แล้ว hand-off จะไม่มีอะไรค้าง

*ต่อจาก 03 (ADR-020), 04 (traceability), 06 (contracts role) — จึง block ไว้*

## Answer

**ทำเสร็จ (AFK task — สเปคการทำ, ยังไม่ได้ commit ไฟล์จริงเพราะ Map นี้เป็น Spec only):**

- **CI:** spec ` .github/workflows/nextjs.yml` (ใหม่) — `on: push/pull_request paths: [src/**, Plan/**, adr/**, docs/**]` jobs: `lint (npm run lint)` + `build (npm run build)` — แทน `erpx-targets.yml` ที่จะติด banner บนหัว `Deprecated — Rust CI, see backup/rust-2026-09-02, retained for history (f97b74d6)` ไม่ลบไฟล์ (ให้ history คงอยู่)
- **Traceability:** spec แก้ `Plan/traceability.md` — หัวตารางเพิ่ม `Next.js route` + `Status` (Archived/Living), ทุกแถว `crates/proto` → แก้เป็น `docs/contracts/proto (archive, 37 proto)` + `src/app/<module> (TBD)` + ติด banner แดงบนหัว “Archived — Rust paths deleted d15909bb, see docs/README + backup”
- **Ops/Fixtures:** `ops/runbooks/*.md` ติด banner `Deprecated — Dioxus-era (cargo xtask verify-web-production) — see backup` ไม่ลบ, `fixtures/` คงเป็น reference (5 ไฟล์, ไม่ต้องแก้), สัญญา `@-mention` คงตาม `Plan/01` แต่ impl รอ Next.js — ไม่ต้องสร้าง ticket ใหม่
- **Result ที่ hand-off ต้องทำ:** 3 ไฟล์ใหม่/แก้ (`nextjs.yml`, `traceability.md`, `ops/runbooks` banners) + 1 ป้าย Deprecated (`erpx-targets.yml`) — ขนาดงาน 1 session, ไม่ต้องตัดสินใจเพิ่ม

*Task นี้ unblock การ hand-off — ไม่มีอะไรต้องตัดสินใจต่อ*

## Comments

- Resolved: 2026-09-02 — AFK, บันทึกสเปคไฟล์ที่จะสร้าง/แก้ ไม่ได้ commit จริง — hand-off จะทำ 1 commit ตามสเปคนี้
- หมอก `ops/runbooks` + `CI` + `traceability` จบแล้ว — หมอกที่เหลือคือ ไทย/อังกฤษ (ยังไม่คมพอ)
