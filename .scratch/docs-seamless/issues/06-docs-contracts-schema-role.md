Status: open
Type: grilling
Blocked by: 01, 03

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

<!-- หลังคุย: เลือก A/B/C + ข้อความแบนเนอร์ + การแก้ CONTRACT_POLICY + ถ้า B ต้องระบุเครื่องมือ -->

## Comments
