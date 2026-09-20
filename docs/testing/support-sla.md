# Support — เวลาทำการ เป้าหมายการแก้ไข และการส่งต่อเมื่อผิดนัด

ขอบเขตนี้เป็น resolution target ของ ticket ที่ผูก quotation/sales order เดิม ไม่ใช่การยืนยันว่าลูกค้ารับบริการแล้ว การรับรองลูกค้าคงใช้ flow ใน [Support tickets](support-tickets.md) ส่วน Portal/Email จริงยังไม่ผ่าน acceptance

## ผู้ทำงานและขั้นตอน

1. Company Support manager ร่าง policy ที่หน้า `/support/sla`: timezone, วันทำงาน, เวลาเปิด–ปิดในวันเดียวกัน, วันหยุด, จำนวนนาทีทำงาน และให้พักนาฬิกาขณะรอลูกค้าหรือไม่ ระบุเหตุผล ไม่ใช้ค่าองค์กรจริงจาก fixture
2. ผู้จัดการอีกคนได้รับ Work/Inbox ตรวจและอนุมัติ; ผู้ร่างอนุมัติตัวเองไม่ได้ ผู้หมดสิทธิ์หรือ engagement ไม่ active ไม่มีสิทธิ์รับงาน การไม่มี reviewer แสดง blocked reason ใน Work ต้องให้ผู้ดูแลจัด coverage
3. ผู้จัดการผูก approved policy กับเคสที่ยัง open โดยตรวจ version ทั้งเคสและ policy นาฬิกาเริ่ม ณ เวลาผูก ไม่คาดเดาว่าข้อมูลเก่าเริ่ม SLA เมื่อใด Snapshot ไม่เปลี่ยนตามการ retire policy ภายหลัง
4. Support agent เข้างานตนจาก Work; Calendar แสดง dueAt จริงตาม timezone ที่เลือก `wait-customer` พักตาม policy, `resume` ใช้เวลาคงเหลือ, `resolve` หยุดนาฬิกา `reject-resolution`/`reopen` ใช้เวลาคงเหลือ ไม่เริ่มงบเวลาใหม่ และไม่ล้าง breach
5. Worker ตรวจทุก 30 วินาที (หยุดเมื่อ maintenance) และเก็บ breach ที่ตรวจพบถาวรหนึ่งรายการต่อเคส Worker retry/พร้อมกันไม่เพิ่ม alert หรือ Inbox ของผู้รับคนเดิมซ้ำ หาก downtime แล้วเคสถูก resolve ภายหลังยังอ่าน breachedAt ที่บันทึกใน owner clock ได้
6. ผู้จัดการที่ eligible เห็น escalation ใน Work/Inbox/หน้า watch ระบุแผนติดตามและรับผิดชอบได้ การ acknowledge ไม่ resolve เคส ไม่เปลี่ยน version/status/clock ของ ticket และไม่ลบประวัติผิดนัด หากไม่มีผู้จัดการ eligible แสดงคิว blocked และ sweep ครั้งถัดไปแจ้งผู้รับใหม่เมื่อได้รับสิทธิ์ถูกต้อง

## ความถูกต้องและขอบเขตสิทธิ์

- เวลานับด้วย UTC instant ตามปฏิทิน local รวมวินาทีและช่วง DST ที่ข้าม/ซ้ำวันหยุด; ไม่ใช้การบวกชั่วโมงแบบ fixed offset
- รองรับช่วงทำงานภายในวันเดียว ไม่รองรับกะคร่อมคืน/หลายช่วงต่อวัน; calendar ที่ใส่เป้าหมายไม่พอใน 366 วันถูกปฏิเสธ
- State/evidence/receipt เปลี่ยนใน transaction เดียว ใช้ idempotency และ expectedVersion; policy event, calendar snapshot และ acknowledged escalation ถูกเก็บ immutable
- Policy เป็นระดับ Company; เคส/alerts จำกัด Company/Branch/Department; counts และ Work ใช้สิทธิ์เจ้าของข้อมูล ผู้จัดการสาขาไม่สร้าง Company policy
- ไม่ส่ง Email จริงในชุดนี้ ไม่มีการอ้างว่าลูกค้าได้รับแจ้ง SLA หรือว่าผ่าน provider gate

## Operator

ใช้ native jobs service ที่มีอยู่ (`scripts/local/jobs.ts` → `supportWork` → `drainSupportSla`) ระหว่างจำลองให้ใช้ฐานทดสอบแยกเท่านั้น ตรวจ event `support_sla_iteration_failed` และรัน worker อีกครั้งหลังแก้ต้นเหตุ ข้อมูลต้นทางยังอยู่ในฐานและ sweep ทำซ้ำได้ คิว watch แสดงรายการไม่รับผิดชอบและกรณีไม่มี eligible manager โดยไม่ข้ามสิทธิ์

## สิ่งที่ยังต้องต่อ

ยังไม่มี default policy binding ตามประเภทบริการ/effective date, first-response target, escalation หลายระดับ, customer-visible SLA/Email, การย้ายผู้รับผิดชอบ escalation ที่ acknowledge ไปแล้ว และ calendar กะคร่อมคืน จึงไม่ใช่ Support/S10/S11/S20 ที่ผ่านครบทั้ง scenario งานรับรอง policy จริงและหลายแผนกลูกค้าต้องทำ UAT ต่อ
