Status: open
Type: prototype
Blocked by: 02, 03, 04, 05, 06, 07

# 08 — ต้นแบบสารบัญ Seamless

## Question

ทำ **ต้นแบบสารบัญ (cheap prototype) ของชุด docs ที่ seamless แล้ว** เพื่อให้ทีมเห็นแล้ววิจารณ์ได้ทันที — ไม่ใช่ของจริง แค่โครงร่าง/ตัวอย่างหน้าที่แสดงว่าหลัง prune จะเดิน docs อย่างไรไม่หลง

วิธีเรียก `skill("prototype")`:
- อ่าน `Prototype` skill ก่อน แล้วสร้าง artifact ขี้เหร่แต่พอเห็นภาพ: อาจเป็น `docs/TOC.md` ร่าง, `Plan/README` ร่าง, หรือ `src/app/docs/page.tsx` stub ที่เรนเดอร์สารบัญ, หรือแค่ markdown outline 1 หน้า
- เลือกตัวที่เห็นภาพสุด: เสนอต้นแบบต้นไม้ 3 ชั้น:
  ```
  README.md (entry)
    ├─ Plan/README → 01-vision → 02-capabilities → traceability (+ลิงก์ไป docs/contracts)
    ├─ CONTEXT.md (ศัพท์เดียว) ← Plan/glossary redirect
    ├─ adr/ (19 → เหลือที่ reconfirmed, ที่ตกยุคมี banner Deprecated)
    ├─ docs/ (contracts/proto archive + schema archive + TOC)
    ├─ Designed/ (Pen canvas — ชี้ด้วย Pencil MCP เท่านั้น)
    ├─ audits/ (สารบัญเดียวหลัง prune → CURRENT-CERTIFICATION + ลิงก์ที่เหลือ)
    └─ fixtures/ops/ (runbooks ใหม่)
  ```
- ผูกกับตั๋วก่อนหน้า: ต้องใช้คำศัพท์จาก 02, ADR จาก 03, Plan จาก 04, audits จาก 05, contracts จาก 06, entry จาก 07 มาประกอบ — จึง block ไว้มาก

HITL — ต้องพรีวิวให้คนดูแล้วตอบว่า “อันนี้แหละ” หรือ “แก้ตรงนี้” ก่อนจะเอาไปทำจริง

## Answer

<!-- หลังทำ prototype: ลิงก์ไฟล์ prototype + สรุป feedback -->

## Comments
