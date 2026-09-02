Status: open
Type: grilling
Blocked by: 01

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

<!-- หลังคุย: ตาราง 19 แถว Reconfirmed/Deprecated พร้อมเหตุผล + ลิงก์ -->

## Comments
