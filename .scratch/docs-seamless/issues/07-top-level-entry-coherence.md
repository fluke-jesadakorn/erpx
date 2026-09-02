Status: open
Type: grilling
Blocked by: 01, 02, 03, 04, 06

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

<!-- หลังคุย: สรุปลำดับนำทาง + ข้อความ CUTOVER entry ใหม่ + ตัดสินใจ CLAUDE.md ว่าจะยุบหรือไม่ -->

## Comments
