Status: resolved
Type: research
Claimed-by: muse-spark-1.2
Resolved-at: 2026-09-02

# 01 — สำรวจ + ตารางความซ้ำ/ย้อนแย้ง

## Question

สำรวจ repo ปัจจุบันบน `development` ทั้งหมดแบบครบถ้วน แล้วสร้าง **ตารางความซ้ำ/ย้อนแย้ง (Contradiction Matrix)** ที่ทำให้เห็นว่า docs ตรงไหนทับกัน ตรงไหนพูดขัดกันเอง เพื่อให้ตั๋วอื่นตัดสินใจได้ — งานนี้คือหาข้อเท็จจริง ไม่ใช่ตัดสินใจ

สิ่งที่ต้องสำรวจ (breadth-first, ไม่ต้องลึกเกินหนึ่ง session):
- ทุก markdown: `Plan/` (01-vision, 02-capabilities 15 กลุ่ม/31 contexts, 03-cross-cutting/@-and-ai 33 ไฟล์, traceability.md, glossary.md, README), `Plan.legacy-2026-08-29/` (16 ไฟล์), `Plan.legacy-codebase-2026-08-29/` (12 ไฟล์), `adr/` (21 ไฟล์ ADR-001..019), `audits/` (330 โฟลเดอร์ + CURRENT-CERTIFICATION.md), `Designed/AGENTS.md` + `MainDesign.pen`, `Delivery/README.md`, `CONTEXT.md`, `CONTRACT_POLICY.md`, `CUTOVER.md`, `README.md`, `AGENTS.md`, `CLAUDE.md`, `.gitignore`, `fixtures/` (ai/ + migration/), `ops/` (runbooks + web-production-evidence), `.agents/` vs `.claude/` (6 skills ซ้ำ), `skills-lock.json`, `docs/` (contracts/proto 37 ไฟล์ + schema 28 ไฟล์), `src/` (Next.js app), `.github/workflows`
- หัวข้อที่รู้ว่าทับกันแน่ๆ: ศัพท์ (`CONTEXT.md` vs `Plan/glossary.md` vs `Plan.legacy*/glossary.md`), CUTOVER สองยุค (2026-07-31 ลบ Next.js→Rust vs 2026-09-02 ลบ Rust→Next.js), `README.md`↔`AGENTS.md`↔`CLAUDE.md` ทับกัน, `CONTRACT_POLICY.md` protobuf vs Next.js REST, `Plan/traceability.md` ยังชี้ `crates/proto` ที่ถูกลบ, `Delivery/README.md` ยังเล่า DDD/Rust, `adr` ยังอ้าง Dioxus/CQRS, `ops/runbooks` ยังอ้าง `cargo xtask`, `.agents/`ซ้ำ`.claude/`, รูป `audits/finance_darkmode` vs `finance_darkmode_fixed` ซ้ำ, `*.log` ที่ ignore แต่ยังอยู่
- สรุปเป็นตาราง: | หัวข้อ | ไฟล์ A | ไฟล์ B | ลักษณะซ้ำ/ย้อนแย้ง | ระดับรุนแรง | ลิงก์บรรทัด |

งานนี้ resolve โดย subagent ที่เรียก `skill("research")` — ไม่ต้องคุยกับคน ให้อ่านไฟล์จริงแล้วเขียนผลลง branch `research/inventory-contradiction-matrix` พร้อม context pointer กลับมาที่ตั๋ว

## Answer

Resolved AFK — research subagent สร้าง branch `research/inventory-contradiction-matrix` @ `f4727aae` ไฟล์ `docs/research-01-matrix.md` (122 บรรทัด)

สาระสำคัญ (5 bullets):
- **Second cutover invisible (critical 7/28 rows):** `CUTOVER.md` มีแค่ 2026-07-31 ลบ Next.js→Rust แต่ `README.md:7`+`docs/README.md:5`+`git log f97b74d6..29136c81` บันทึก reverse 2026-09-02 Rust→Next.js — ทำให้ `CONTRACT_POLICY.md` (protobuf), `Plan/traceability.md` (558× `crates/proto` dead links), `Delivery/README.md` 768 บรรทัด Rust, `ops/runbooks`, `.github/workflows` (Rust CI), `audits/CURRENT-CERTIFICATION.md` (Dioxus cert) กลายเป็น stale critical ทั้งหมด
- **Plan 3 ชั้นซ้อน:** `Plan/` 68 md + `Plan.legacy-2026-08-29/` 43 md + `Plan.legacy-codebase-2026-08-29/` 40 md — ผู้อ่านต้องเลือก glossary 3 ที่ (`CONTEXT.md` 46 บรรทัด vs `Plan/glossary.md` pointer 5 บรรทัด vs legacy 51 บรรทัด) และ context 3 ชั้น
- **audits 72M/330dirs/324md ซ้ำหนัก:** `finance_darkmode` 2.7M ≡ `finance_darkmode_fixed` 2.7M, `finance_images` 852K, `finance_live*` 4 ชุด, `finance-e2e-2026-08-30.log` 431K ยังอยู่ทั้งที่ `.gitignore:26:*.log` ignore, `CURRENT-CERTIFICATION.md` บอก cargo gate PASS ทั้งที่ `README.md:55` บอก obsolete
- **`.agents` ≡ `.claude`:** 6 skills ×2 = 30 ไฟล์ซ้ำ 100% ต่างแค่ `.barlevalon-installed` 5 ไฟล์, `skills-lock.json` hash ตรงแต่ไม่มี CI ตรวจ drift, `AGENTS.md` มี 2 fences machine-owned (nextjs-rules + barlevalon)
- **docs archive orphaned:** `docs/README.md` บอก reference-only ถูกต้อง แต่ `CONTRACT_POLICY.md` ยังบังคับ protobuf + `Plan/traceability` ไม่ชี้ไป `docs/contracts/proto`

Branch: `research/inventory-contradiction-matrix` — link: `docs/research-01-matrix.md` (ดู `git show research/inventory-contradiction-matrix:docs/research-01-matrix.md`)
Frontier ปลดล็อก: 02,03,04,05,06 (และ 07 บางส่วน) ทะลุ `Blocked by: 01` แล้ว

## Comments

- Resolved: 2026-09-02 via research subagent (AFK, no HITL) — facts only, decisions remain for tickets 02–09
