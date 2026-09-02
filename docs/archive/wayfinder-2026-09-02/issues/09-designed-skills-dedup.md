Status: resolved
Type: research
Claimed-by: muse-spark-1.2
Resolved-at: 2026-09-02

# 09 — Designed + Skills ซ้ำซ้อน

## Question

หาข้อเท็จจริงว่า **จะเลิกซ้ำอย่างไร** ระหว่างสองคู่: `Designed/MainDesign.pen` (+ `Designed/AGENTS.md` ที่ห้าม grep `.pen`) กับ docs ตัวอักษร, และ `.agents/skills/` ซ้ำกับ `.claude/skills/` (6 skills เดียวกัน: domain-modeling, grilling, prototype, research, wayfinder, setup-matt-pocock-skills)

หัวข้อสำรวจ (breadth-first, ไม่ต้องตัดสินใจตอนนี้):
- อ่าน `Designed/AGENTS.md`, `Designed/MainDesign.pen` (ผ่าน Pencil MCP เท่านั้น — ห้าม `read` ไฟล์ `.pen` โดยตรง), ดูว่า docs ไหนที่เลียนแบบ canvas อยู่แล้ว (เช่น `Plan/02` เคยลิงก์ไป Design ไหม)
- เทียบ `.agents/skills/*/SKILL.md` vs `.claude/skills/*/SKILL.md` — ซ้ำ 100% หรือต่างกันตรง `.barlevalon-installed`?
- เทียบ `.github/copilot-instructions.md` vs `AGENTS.md` vs `CLAUDE.md` — ซ้ำแค่ไหน
- สำรวจว่า Next.js scaffold สร้าง `CLAUDE.md` (`@AGENTS.md`) + `AGENTS.md` (nextjs-rules) แยกกัน — ควรยุบไหม

Resolve โดย subagent `skill("research")` — อ่านไฟล์จริง, เขียนผลลง branch `research/designed-skills-dedup` พร้อมตารางซ้ำ + แนะนำวิธียุบ (เช่น เหลือ `.agents/` เดียว, ให้ `.claude/` เป็น symlink หรือลบ)

## Answer

Resolved AFK — branch `research/designed-skills-dedup` @ `707ac4eb` ไฟล์ `docs/research-09-dedup.md` (118 บรรทัด, ไม่เขียน .scratch ใหม่)

ข้อเท็จจริง:
- `Designed/MainDesign.pen` 16012554 B (15M) version 2.17, 894 top children / 32990 nodes (272 Page · + 204 Action State · + 205 Decision · + 230 Outcome), `images/` 27 PNG รวม 16909762 B (16M) — คนละขนาดกับที่ research-01 นับ 10 รูป; `grep -r "MainDesign" Plan/` = 0 hits — canvas เป็น single source ของ route `/work-queue`, `/billing/*`, `/platform/*` จริง ไม่ได้ซ้ำกับ `Plan/02` ที่ยังอ้าง `crates/proto` (stale)
- `.agents/skills/` ≡ `.claude/skills/` — `diff -r` ต่างแค่ 5× `.barlevalon-installed` (source `mattpocock/skills@6654f6b`, installedAt 2026-09-02T15:39:39.77xZ) นอกนั้น `SKILL.md` + `agents/openai.yaml` + helper MD ตรงกัน 100% (74/28/26/12/128/116 บรรทัด), `skills-lock.json` root เก็บ hash 6 ตัวตรง — แนะนำลบ `.claude/skills` เหลือ `.agents/skills` เดียว (symlink ถ้าต้องคง path)
- `AGENTS.md:1-9` nextjs-rules + `AGENTS.md:76-89` barlevalon skills เป็น machine-owned fences ห้ามแก้ด้วยมือ, `CLAUDE.md:1` `@AGENTS.md` เป็น redirect ถูกต้อง, `.github/copilot-instructions.md:1` เป็น variant ที่ตั้งใจ (ขาดประโยค helper-files)
- คำแนะนำ: เพิ่ม `Designed/README.md` stub + `.gitattributes` `*.pen filter=lfs` ถ้าเกิน 20M; เพิ่ม `npm run verify:skills` เช็ค hash

Branch: `research/designed-skills-dedup` — link: `docs/research-09-dedup.md` (ดู `git show research/designed-skills-dedup:docs/research-09-dedup.md`)

## Comments

- Resolved: 2026-09-02 via research subagent (AFK) — เคารพ `Designed/AGENTS.md` Pencil-only guardrail ไม่ได้ `cat` .pen
