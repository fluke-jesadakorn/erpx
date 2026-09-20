# Dispatch / in-transit / partial receiving — ชุด C

## ขอบเขตที่เพิ่ม

เอกสารโอนเดิมเป็น `delivery_mode=direct` และทำงานเดิมทุกประการ ข้อมูลเก่าไม่ถูกแปลงเป็นประวัติขนส่ง ยอดและ GL เดิมไม่ถูก repost

เอกสารใหม่เลือก `transit` พร้อมวันคาดว่าจะถึง: ผู้เตรียมต้นทาง → ผู้อนุมัติอิสระตาม Flow เดิม → ผู้ส่งคลังต้นทาง → ผู้รับคลังปลายทาง → Finance เมื่อมีของขาด

1. สร้างและ submit transfer ปริมาณ/ต้นทุน snapshot ต้นทาง ผ่านผู้รับผิดชอบตาม Flow จริง ผู้อนุมัติเป็นคนละคนกับผู้สร้าง/ผู้เตรียม
2. อนุมัติสร้าง `inventory_transit` และ lines ใน transaction เดียวกันกับ source approval โดยยังไม่ตัดสินค้าหรือเพิ่มปลายทาง สถานะ `posted` ของ parent สำหรับ mode นี้หมายถึงอนุมัติ transport; หน้าจอแสดง Transport approved และลิงก์ไปงานขนส่ง
3. ผู้มี `inventory.prepare` ต้นทางยืนยันออกจริง พร้อมวันที่ tracking/reference และ evidence จึงตัด stock เข้าบัญชีสินค้าระหว่างทาง ต้องยังตรง source stock version/quantity/value ที่อนุมัติ ไม่กินยอดจองหรือฝ่า quality hold
4. ปลายทางรับบางส่วนได้โดยบุคคลอื่นที่ไม่ใช่ผู้เตรียมหรือผู้ส่ง รายการเดิมจองผลต่างได้แต่ห้ามรับเกินยอด ผลรับเข้าสต็อก/ต้นทุน/events/receipt/audit/outbox สำเร็จพร้อมกัน
5. รายงานของขาดยังไม่ลด inventory asset: quantity ถูกกันออกจากยอดรับได้ แต่ยังอยู่ใน `inventory_in_transit` จน Finance อิสระอนุมัติ หลังตรวจหลักฐาน เลือกงวดเปิดและบัญชีค่าเสียหาย จึงลง Dr expense / Cr inventory ด้วยบัญชีต้นทางที่ snapshot ไว้
6. Finance ปฏิเสธผลต่างได้ ยอดกลับไปให้ผู้รับตรวจรับต่อ ประวัติข้อเสนอและการตัดสินใจแก้ทับไม่ได้
7. ยกเลิกได้เฉพาะก่อนออกจริง พร้อมเหตุผลและหลักฐาน เมื่อส่งแล้วต้องรับ/แก้ผลต่าง ห้ามลบหรือ cancel เพื่อทำให้สินค้าระหว่างทางหาย

## มูลค่า สิทธิ์ และ handoff

- ปัดเศษแบบ cumulative allocation จากจำนวนและมูลค่าที่อนุมัติ เก็บ integer milli-units / minor currency units เดิม ทุกส่วนสุดท้ายต้องรวมกลับเท่าต้นทุนเริ่มต้น
- `inventory_in_transit` แสดง quantity/value ที่ยังไม่รับหรือยืนยัน loss; Commerce stock แสดงมูลค่าระหว่างทางแยก currency ไม่ปะปนกับ stock พร้อมใช้ และ movement history ลิงก์กลับ owner ขนส่ง
- รับเข้าคลังที่กักสินค้าได้ แต่ quality hold ยังอยู่ สินค้ารับเข้าจะขาย/ส่งออกไม่ได้จนผ่าน release เดิม
- Company/Branch ต้องเป็นต้นทางหรือปลายทาง Department ไม่เห็นข้อมูล; Finance loss reviewer ต้องใช้ Company scope และเป็นคนละคนกับผู้ส่งและผู้เสนอผลต่าง
- Work/Inbox/Calendar อ่าน source เดียวกันและตรวจสิทธิ์ source; queue แสดง source dispatch, receiving หรือ independent Finance ตามงานที่ค้าง มี blocked reason หากไม่มีผู้รับ eligible
- ยังใช้ team queue และวันคาดว่าจะถึง ไม่ใช่ named reassignment/SLA business-calendar snapshot หรือ timed escalation
- ใช้ Idempotency-Key / expectedVersion / active assignment guards กับทุกคำสั่ง; immutable events เก็บแหล่งสินค้า ผลรับ ผลต่าง และ journal
- รับของ/ออกของ/รับรอง loss ย้อนเข้า closed period ไม่ได้ การแก้ accounting configuration ภายหลังไม่เปลี่ยน currency/chart/inventory-account snapshot ของ transport

## การทดสอบและข้อจำกัด

`tests/inventory-transit.test.ts` ใช้ source transfer commands และ Flow assignment/approval จริง ก่อนสร้าง transport; แทรกตรงเฉพาะ master/opening-stock fixtures ตรวจ conservation, rounding, partial receiving, duplicate/stale/race, loss approve/reject/GL, scope, revoked role, closed period, quarantine, Work/Inbox/Calendar และ direct-transfer compatibility

ยังไม่ใช่ intercompany transfer, lot/serial tracking, carrier integration, customer shipment/POD หรือ supplier partial receiving รอบนี้ไม่ปลด external/provider/UAT/production gates และไม่ถือว่าปิดชุด C หรือ A–J ทั้งหมด
