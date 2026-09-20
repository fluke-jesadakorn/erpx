# Quality hold — ชุด C (จำลองบางส่วน)

## ขอบเขตที่พัฒนา

หน้า `/holdings/{holdingSlug}/quality` และ API `/api/v1/holdings/{holdingSlug}/quality` เพิ่ม owner service สำหรับกักสินค้าทั้งชนิดในคลังเดียวกัน ไม่ใช่ราย lot/serial และไม่เปลี่ยนจำนวนหรือมูลค่าสินค้าเดิม

1. ผู้มี `quality.hold` เลือกสินค้า/คลัง ระบุเหตุผลและวันนัดตรวจ พร้อม stock version ปัจจุบัน
2. ผู้มี `quality.inspect` ที่เป็นคนละคนกับผู้สร้าง บันทึก pass/fail พร้อม evidence ข้อความ ระบบเก็บผลตรวจเดิมเป็น immutable rows; fail ยังคงกักและตรวจซ้ำได้
3. ผู้มี `quality.release` ที่เป็นคนละคนกับทั้งผู้สร้างและผู้ตรวจล่าสุด ปล่อยเฉพาะ hold ที่ผลล่าสุดผ่าน เมื่อมี hold ซ้อนกันต้องปล่อยทุกใบก่อนนำสินค้าออก

ไม่มีการยกเลิกเพื่อข้ามการตรวจ หากกักผิดต้องตรวจผ่านและปล่อยอย่างมีหลักฐาน ยังไม่มี workflow scrap, supplier return disposition หรือการยกเลิกที่ได้รับรอง

## ความถูกต้องและสิทธิ์

- Migration `0206_quality_holds.sql` เป็น additive พร้อม roles `quality_inspector` / `quality_manager`; Company/Branch เท่านั้น Department ไม่เห็นข้อมูล
- คำสั่งใช้ Idempotency-Key / expectedVersion, current assignment guards, audit, command receipt และ outbox เดิม; ไม่สร้าง approval ย้อนหลัง
- ทุกการเปลี่ยน hold แตะ stock version เพื่อแข่งกับคำสั่งจอง/ส่งของอย่างปลอดภัยใน transaction แบบ serializable
- DB guard กัน stock quantity ลดจาก Sales, POS, transfer, count adjustment และ owner อื่นที่เขียน `commerce_stock`; reservation ใหม่หรือเพิ่มยอดถูกกันด้วย
- จองไว้ก่อนเกิด hold ก็ส่งไม่ได้: consumption/payment ที่ล้มเหลว rollback ทั้ง transaction ยอดจองและยอดเงินเดิมต้องยังอยู่ การ release reservation ทำได้
- รับสินค้าเพิ่มได้ แต่สินค้าที่รับเพิ่มอยู่ภายใต้ hold ของ warehouse/product เดียวกัน การเพิ่มมูลค่าหรือรับเข้าไม่ใช่การปล่อย hold
- Quality stock selector ส่งเฉพาะ stock IDs, จำนวน, version และชื่อ ไม่ส่ง inventory cost ให้ผู้ตรวจ
- POS และ Commerce stock แสดง quality hold แยกจากยอด reservation ไม่หักซ้ำเป็นจำนวนจอง
- Work/Inbox/Calendar อ้างอิง hold ต้นทางและกรอง scope; ทีมตรวจ/อนุมัติมาจาก active eligible assignments รวม custom roles ที่มีสิทธิ์ ไม่เลือกคนแทนเงียบ ๆ
- ถ้าไม่มีผู้รับที่เป็นอิสระแสดง blocked reason ให้ Company administrator จัดสิทธิ์ผ่าน IAM; ยังไม่มี timed escalation หรือ named reassignment history สำหรับ quality
- Calendar ใช้วันนัดแบบ all-day ตาม timezone ที่เลือก; Work ใช้วันที่ UTC สำหรับ dueAt ยังไม่มี Company business-calendar/SLA policy snapshot จึงยังไม่ปิดเกณฑ์ A

## วิธีตรวจและข้อจำกัด

- `tests/quality.test.ts`: fail/reinspect/pass/release, independent actors, immutable evidence, duplicate/stale/revoked, Company/Branch/Department scope, overlapping holds, reserved Sales fulfillment rollback, Work/Inbox/Calendar และ quarantine/reservation race
- `tests/pos.test.ts`: reserved retail payment ขณะถูกกักต้องไม่มี payment/receipt ใหม่ และ stock/value คงเดิม
- รัน owner regression ร่วมกับ reservation, IAM, Calendar, navigation และ schema migration บนฐานข้อมูลทดสอบแยก
- ตรวจหน้าคิวและฟอร์มด้วย session ที่ได้รับอนุญาตผ่าน public development hostname; ยังไม่ได้ทำ browser acceptance หลายบทบาทหรือธุรกรรม browser จนครบ
- ช่องว่างที่ยังเหลือ: lot/serial inspection, structured measurements, private attachments, scrap/return disposition, cancellation, timed escalation, Company policy snapshot, provider/production และการรับรองธุรกิจ

ผลนี้ไม่ใช่การปิดทั้ง Quality capability, scenario S03/S04/S11 หรือชุด C ทั้งชุด ดูผลและ checksum ใน acceptance evidence ledger
