Status: resolved
Type: grilling
Blocked by: 01, 02, 03, 04, 06
Claimed-by: muse-spark-1.2
Resolved-at: 2026-09-02

# 07 — หน้าแรกนำทางให้ตรงกัน

## Question

ตัดสินใจให้ **หน้าแรก 5 ไฟล์นำทางตรงกัน ไม่พูดขัดกัน**: `README.md` (เพิ่งเขียนใหม่เป็น Next.js reboot), `AGENTS.md` (มี nextjs-agent-rules + barlevalon skills), `CLAUDE.md` (`@AGENTS.md` เดียว), `CUTOVER.md` (ยังมีแค่ cutover 2026-07-31 Rust ที่ลบ Next.js เก่า ยังไม่มี 2026-09-02 ที่ลบ Rust กลับมา), `CONTRACT_POLICY.md` + `.env.example` + `skills-lock.json` + `.github/workflows` (ยังเป็น Rust `erpx-targets.yml`)

หัวข้อตัดสินใจ (แตกเป็น checklist ย่อยในตั๋วเดียว):
- `CUTOVER.md` จะเพิ่ม entry 2026-09-02 “Rust→Next.js reverse cutover” อย่างไร — recovery boundary คือ `f97b74d6` + `rust-archive-2026-09-02` + `cc0887d` หรือแค่ชี้ไป `docs/README.md`
- `CLAUDE.md` จะคงไว้เป็น redirect หรือยุบรวมเข้า `AGENTS.md` ให้เหลือไฟล์เดียว
- `AGENTS.md` ส่วน Tooling/Verification/Structure จะล็อกเป็น `npm run dev/build/lint` + `src/app/` จริงไหม จะใส่ recovery note ไป backup อย่างไร
- `README.md` ส่วน Stack/Structure/Verification จะให้ลิงก์ไป `Plan/` + `CONTEXT.md` + `adr/` ลำดับไหน
- `CONTRACT_POLICY.md` + `.env.example` จะแก้ให้ตรง Next.js (เช่น ลบ `POSTGRES_HOST` แบบ Rust หรือคงไว้เป็น template)
- `.github/workflows` จะทำเป็น Next.js workflow หรือติดป้าย Deprecated

เรียก `grilling` + `domain-modeling` — ต้องคุยให้ 5 ไฟล์เล่าเรื่องเดียวกัน คำเดียวกัน

## Answer

**ตัดสินใจ (checklist 6 ข้อ — grilling):**

- **`CUTOVER.md` เพิ่ม entry 2026-09-02 “Rust→Next.js reverse cutover”:** ใส่บนสุดหลัง header ว่า `Cutover completed on 2026-09-02: hard-cut Rust monorepo (137 crates, 1321 rs) to Next.js 16 App Router` + recovery `f97b74d6` + tag `rust-archive-2026-09-02` + baseline `cc0887d` + ลิงก์ `docs/README.md` + ระบุว่า `docs/contracts/proto` + `docs/schema` เป็น archive; เก่า 2026-07-31 คงไว้ใต้หัว “## 2026-07-31 Folio→Rust” เพื่อไม่ลบประวัติ
- **`CLAUDE.md` คงเป็น redirect `@AGENTS.md` เดียว:** ไม่ยุบ — Claude Code ใช้ `CLAUDE.md` โดยเฉพาะ ส่วน `AGENTS.md` ใช้ opencode/nextjs-rules; ปล่อยแยกถูกต้องแล้ว (ตาม research-09)
- **`AGENTS.md` ล็อกเป็น Next.js แล้ว** (`nextjs-agent-rules` + `barlevalon` fences ห้ามแก้ด้วยมือ) — คง Tooling `npm run dev/build/lint`, Structure `src/app/`, Recovery ชี้ backup/shell ตามที่ `29136c81` ทำแล้ว; ไม่ต้องแก้อีก
- **`README.md` ล็อกเป็น Next.js reboot แล้ว** (`29136c81`) — ลำดับ Stack→Recovery→Structure→Verification→Plan/adr/Designed ชัดแล้ว; แค่ให้ `README` ลิงก์ไป `Plan/README`, `CONTEXT.md`, `adr/` ตามลำดับที่ 02+03 ตัดสิน
- **`CONTRACT_POLICY.md` แก้เป็น Next.js ตาม 06:** เปลี่ยน 6 bullets protobuf → REST/tRPC + Prisma (field never reused, idempotency + tenant scope, update OpenAPI + schema)
- **`.env.example` + `.github/workflows`:** `.env.example` เปลี่ยน header เป็น Next.js `NEXT_PUBLIC_*` + comment ว่า Rust vars เก่า archive ใน `docs/schema`; `.github/workflows/erpx-targets.yml` ติดป้าย `Deprecated — Rust CI, see backup/rust-2026-09-02` + เพิ่ม `nextjs.yml` ใหม่ `npm run lint + build` ที่ trigger `src/**` (จะสร้างเป็น ticket 10)

*7 ข้อนี้ทำให้ 5 ไฟล์เล่าเรื่องเดียวกัน (Next.js reboot 2026-09-02 + recovery 3 ทาง)*

## Comments

- Resolved: 2026-09-02 — checklist 6 ข้อครบ; ปลดล็อก 08 ทั้งหมด
- ต่อจากนี้ `CUTOVER.md` จะมี 2 entries ชัดเจน ไม่ขัดกันอีก
