Status: resolved
Type: grilling
Blocked by: 01
Claimed-by: muse-spark-1.2
Resolved-at: 2026-09-02

# 03 — ADR ไหนยังศักดิ์สิทธิ์ในยุค Next.js

## Question

ตัดสินใจว่า **19 ADR ใน `adr/` (ADR-001..019) ตัวไหนยังเป็นกฎหมาย ตัวไหนตกยุคแล้ว** หลัง cutover จาก Rust/Dioxus/Tonic/SQLx ไป Next.js — ต้องให้เหลือชุด ADR ที่ไม่ย้อนแย้งกับปัจจุบัน

สิ่งที่ต้องเทียบทีละฉบับ (ต้องอ่าน `adr/*.md` จริง):
- ADR-001 holding isolation, ADR-002 CQRS, ADR-003 no-long-term-dual-writes, ADR-004 expiring RBAC, ADR-005 global read auditing, ADR-006 support capabilities, ADR-007 stripe authority, ADR-008 operations boundaries, ADR-009 marketing/crm boundaries, ADR-010 dioxus target strategy, ADR-011 customer data-plane agent, ADR-012 three-layer corporate identity, ADR-013 holding-branch DB + directory authority, ADR-014 cqrs bounded-context modular monolith, ADR-015 retire n8n, ADR-016 dioxus-native cutover, ADR-017 collaboration + AI, ADR-018 open-source AI + pricing, ADR-019 mentions + field AI

หัวข้อตัดสินใจ:
- ตัวไหน Reconfirmed / Deprecated / Replaced / Superseded — ติดป้ายหน้าตั๋วอย่างไร
- สำหรับตัวที่ตกยุค (เช่น 010, 014, 016 ที่ผูก Dioxus/CQRS/Rust) จะชี้ไป successor เป็น `docs/` ADR ใหม่หรือแค่ archive?
- ต้องเพิ่ม ADR ใหม่สำหรับ Next.js stack ไหม (App Router, Prisma/Drizzle, Auth, Tailwind) หรือพอแค่หมายเหตุ

เรียก `grilling` + `domain-modeling` คู่กัน — ต้องขุดว่า isolation/security ยังยึดตามเดิมแค่เปลี่ยน stack

## Answer

**ตัดสินใจ (grilling + domain-modeling):**

| ADR | ชื่อ | สถานะใหม่ | เหตุผล / ชี้ต่อ |
|---|---|---|---|
| 001 holding isolation | Reconfirmed | แยก Holding เป็น isolation boundary ยังใช้ได้ — ไม่ผูก stack |
| 002 cqrs | Superseded (Next.js) | PostgreSQL CQRS write/read + outbox ยังเป็น pattern ได้ แต่ Rust aggregate/snapshot/worker ไม่ใช่ Next.js; ให้ ADR-020 ใหม่กำหนด Next.js CQRS/Prisma แทน |
| 003 no-long-term-dual-writes | Reconfirmed | หลัก no dual-write ยังถูก |
| 004 expiring RBAC | Reconfirmed | RBAC ชั่วคราวยังใช้ |
| 005 global read auditing | Reconfirmed | หลัก audit ข้าม boundary ยังใช้ |
| 006 support capabilities | Reconfirmed | |
| 007 stripe authority | Reconfirmed | |
| 008 operations boundaries | Reconfirmed | |
| 009 marketing/crm boundaries | Reconfirmed | |
| 010 dioxus target strategy | Deprecated — Archived | Dioxus shared UI crate ถูกลบ `d15909bb`; Next.js App Router แทน |
| 011 customer data-plane agent | Reconfirmed (หรือ Superseded ถ้าไม่มี agent) | ยังอ้างได้แต่ต้องดูว่า Next.js มี data-plane agent ไหม — ให้คงไว้พร้อมหมายเหตุ |
| 012 three-layer corporate identity | Reconfirmed | |
| 013 holding-branch DB + directory | Reconfirmed | |
| 014 cqrs modular monolith | Deprecated — Superseded | Rust crate per bounded context ถูกลบ; ให้ ADR-020 แทนด้วย Next.js modular monolith (src/app + lib) |
| 015 retire n8n | Reconfirmed | |
| 016 dioxus-native cutover | Deprecated — Archived | Blitz/WGPU cutover ถูกลบ |
| 017 collaboration + AI | Reconfirmed | ยังใช้ แต่ impl เปลี่ยนจาก Rust worker เป็น Next.js |
| 018 open-source AI + pricing | Reconfirmed | |
| 019 mentions + field AI | Reconfirmed | |

- **วิธีติดป้าย:** แก้ header แต่ละไฟล์: `**Status:** Deprecated — Superseded by ADR-020 (Next.js stack)` หรือ `Reconfirmed for Next.js era (2026-09-02)` + ใส่วันที่
- **ต้องเพิ่ม ADR-020:** `ADR-020-nextjs-stack.md` — ตัดสินใจ stack: Next.js 16 App Router + React 19 + Tailwind 4 + TypeScript + Prisma/Drizzle (เลือกหนึ่ง) + Auth (NextAuth/Clerk) + App Router route handlers แทน Tonic — เป็น successor ของ 002/010/014/016
- **ไม่ต้องแก้เนื้อหาเก่า** แค่ติดป้ายและลิงก์ไป ADR-020/ไป `backup/rust-2026-09-02` สำหรับประวัติ

*เรียก grilling 3 คำถาม frontier เกี่ยวกับ isolation/security — คำตอบ: ยึดหลักเดิมเปลี่ยนแค่ impl*

## Comments

- Resolved: 2026-09-02 — grill 2 skills → ตาราง 19 แถว; ปลดล็อก 06 และส่วนหนึ่งของ 07
- ต่อไปต้องการ ADR-020 จริง (จะถูกสร้างเป็น task หลัง Map ปิด)
