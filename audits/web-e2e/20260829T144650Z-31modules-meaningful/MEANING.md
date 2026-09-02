# Meaningful E2E Captures — ทำไมรูปเดิมไม่ใช่ระบบใช้งานได้จริง

## ปัญหาของรูปเดิม (shell เปล่า)
- `sign-in-invalid.png` — แค่ negative case (กรอกผิด) ไม่ได้พิสูจน์ login สำเร็จ
- `desktop-en-light.png` — `/work` หลัง login แต่เป็น shell เปล่า ไม่มีตาราง ไม่มีปุ่ม Create ไม่เห็น CRUD
- `mobile-th-dark.png` — `/modules` แค่ list ชื่อ workspace 21 อัน ไม่ได้พิสูจน์ว่ากดเข้า `/manufacturing` แล้วสร้าง Order ได้
- `report.json` เดิมมี `workspace_routes: ["/finance"]` ตัวเดียวจาก 21 ตัว — อีก 20 ตัวไม่ถูกเทส

## รูปที่มีความหมายต้องเป็นแบบนี้ (ต่อ module 4-5 รูป)
ต่อ module (31) ต้องมี:
1. `<ctx>_list.png` — List/Table เห็น `record-row` จริงหรือ `empty-state` ที่ไม่ใช่ shell + `dataset.freshness=fresh` + `route-meta v1`
2. `<ctx>_detail.png` — Detail/Header เห็น ordered `@`-chips + `ListBacklinks` panel ที่มี host กลับไป
3. `<ctx>_form.png` — Form เห็น field list (field_id/kind/required/max) + inline chip picker `@`-prefix กำลังเปิด + validation
4. `<ctx>_ai-ready.png` — Form เดิมแต่มี `FieldSuggestionCard` ghost/diff + Evidence/Provenance/output_digest พร้อม `Apply/Reject`
5. `<ctx>_applied.png` — หลังกด Apply เห็น `route-meta v2` และ `dataset.freshness` ยัง fresh, chip ยังอยู่, backlink ยังอยู่ — ถ้า source เปลี่ยนต้องได้ `Conflicted` แทน

ตอนนี้ 31/31 OwnerPublished เสร็จแล้ว (cargo check เขียว) เหลือรัน `cargo xtask web-e2e` ตอน projectors healthy เพื่อเติม 31×5 รูปให้ครบ — โครงสร้างโฟลเดอร์นี้เตรียมไว้แล้ว
