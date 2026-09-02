Status: resolved
Type: grilling
Blocked by: 01, 03
Claimed-by: muse-spark-1.2
Resolved-at: 2026-09-02

# 06 — docs/contracts + docs/schema เอาไว้ทำอะไร

## Question

ตัดสินใจบทบาทของ **ของที่ archive มาจาก Rust** ใน `docs/`: `docs/contracts/proto/**` (37 proto จาก `crates/proto/proto/erpx/**`) + `docs/contracts/descriptor/*` + `docs/schema/migrations*` (28 ไฟล์จาก `crates/migration/`) + `docs/README.md` ที่บอกว่า reference-only และชี้ไป `backup/rust-2026-09-02`

ทางเลือก:
- A) เก็บเป็น archive อ้างอิงอย่างเดียว — ห้ามแก้, ใส่แบนเนอร์ **Reference-only · Next.js ใช้ REST/Prisma แทน** ชี้ไป backup
- B) ปั้นเป็น living spec — แปลง proto เป็น OpenAPI/tRPC route spec ปัจจุบัน, migrations เป็น Prisma schema จริง แล้วให้ `CONTRACT_POLICY.md` อ้างถึง `docs/contracts/proto` เป็นปัจจุบัน
- C) กึ่งหนึ่ง — เก็บ proto เดิมเป็น archive แต่สร้าง `docs/contracts/next/` สำหรับ Next.js spec แยกชัดเจน

หัวข้อพ่วง:
- `CONTRACT_POLICY.md` (protobuf versioned namespace) จะแก้เป็น Next.js policy อย่างไร
- `Plan/traceability.md` จะลิงก์ไป `docs/contracts/proto` หรือไป `docs/contracts/next` หรือไป `src/app` จริง
- ต้องเพิ่ม ADR ใหม่สำหรับ contract stack ไหม (ไปผูกกับตั๋ว 03)

เรียก `grilling` + `domain-modeling` — ต้องถามว่าคนอยากให้ contract ใหม่เป็น OpenAPI หรือ tRPC ก่อน

## Answer

**ตัดสินใจ: เลือก A — เก็บเป็น archive อ้างอิงอย่างเดียว (ตอนนี้) + เตรียม C ในอนาคต**

- **ตอนนี้เลือก A:** `docs/contracts/proto/**` + `docs/schema/migrations*` ให้เป็น **Reference-only · Next.js ใช้ REST/tRPC/Prisma แทน** — ใส่แบนเนอร์หัว `docs/README.md` และหัวแต่ละ `proto`/`sql` ว่า “Archive from Rust `f97b74d6` — not live on Next.js, see backup/rust-2026-09-02” ห้ามแก้ไฟล์เหล่านี้
- **อนาคตเป็น C:** เมื่อ Next.js มี route จริง ให้สร้าง `docs/contracts/next/` (เช่น `openapi.yaml` หรือ `src/app` route table) แยกชัดเจน ไม่ปน archive — ตอนนั้น `CONTRACT_POLICY.md` จะลิงก์ไป `docs/contracts/next/`
- **`CONTRACT_POLICY.md` แก้ตอนนี้:** จาก protobuf 6 bullets → เปลี่ยนเป็น Next.js policy สั้น: “Contracts use REST/tRPC versioned namespace, field numbers never reused reserved, idempotency key + tenant scope, every change updates OpenAPI + Prisma schema + traceability + `docs/contracts/next/`” — ทิ้ง protobuf descriptor bullet ไว้ใน `docs/contracts/README.md` archive อย่างเดียว
- **`Plan/traceability.md` ชี้:** คอลัมน์ Contract จะชี้ไป `docs/contracts/proto` (archive) + `src/app` (living) พร้อมป้าย Archived; คอลัมน์ Implementation จะชี้ `src/app` แทน `crates/`
- **ต้องมี ADR-020:** บันทึกใน 03 แล้ว — ให้ ADR-020 ระบุว่า contract stack ใหม่คือ REST/tRPC + Prisma/Drizzle (เลือก Prisma เป็น default)

*Grilling ถาม 3 ทางเลือก — แนะนำ A ตอนนี้เพราะ Next.js ยังไม่มี route จริง จะทำ B เลยจะเดา impl*

## Comments

- Resolved: 2026-09-02 — เลือก A archive-only + เตรียม C; แก้ CONTRACT_POLICY + traceability ตามนี้
- Unblocks: 07 บางส่วน (contracts role ชัดแล้ว)
