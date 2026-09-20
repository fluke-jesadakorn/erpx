# Customer 360: scoped source timeline

ชุด D / S01,S02,S04,S09: เพิ่ม projection แบบ read-only จาก owner tables เดิม ไม่สร้างสถานะธุรกิจอีกชุด. หน้า `/customers` แสดงเฉพาะ customer ที่มี quotation/sales หรือ qualified CRM lead ใน Company/Branch/Department ปัจจุบัน. ลูกค้าที่มีอยู่แต่ไม่มี source ที่อ่านได้ไม่ปรากฏใน directory และ detail ตอบ404; ต้องมี commerce.read.

Timeline รวม latest quotation/order, fulfillment, customer return, note correction/archive history, follow-up history และ qualified CRM lead/opportunity history. ทุกแถวมี source link/version/time; event ที่เป็น notes ไม่ส่ง body/reason, CRM ไม่ส่ง contact details/brief/event payload, ไม่มีต้นทุน HR Legal beneficiary หรือไฟล์แนบ. Source details ยังตรวจสิทธิ์เอง. API ใช้ private no-store.

มี keyset pagination ตาม time/id แบบ deterministic; cursor ไม่เพิ่มสิทธิ์และอ่านใหม่ตาม scope ทุกครั้ง. Order/return แสดงสถานะล่าสุด ไม่แอบอ้างว่าเป็นประวัติ transition ทั้งหมด. Directory แสดง200รายการแรกพร้อมแจ้งข้อจำกัด; รายการอื่นเปิดจากต้นทางได้.

การทดสอบสร้างทุก record ผ่าน owner command: quotation → note create/correct/archive, follow-up complete และ lead qualify; ตรวจ source version ไม่ถูกแก้โดยการอ่าน, field allowlist, privacy, scope, pagination และ malformed cursor.

ยังไม่รวม Support/Field Service/Legal, invoice/refund/provider timeline, identity merge, Portal หรือ customer authority. เป็น internal staff projection ไม่ใช่ public customer projection. Capability ยัง partial และ S01–S21 ยังไม่ถือว่าครบ.

ส่วนขยาย Support: อ่าน lifecycle events แบบ field allowlist เมื่อผู้ใช้มี support.read เพิ่มจาก commerce.read เท่านั้น; tests ตรวจว่า purchasing operator ไม่เห็นแม้อ่านเอกสารการค้าได้. Browser ของ Support บันทึกแยกใน ledger.
