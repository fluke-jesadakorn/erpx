# Support — incident ที่เชื่อมเคสลูกค้า

Incident เป็นงานสืบสวนและแก้สาเหตุร่วม ส่วน ticket แต่ละรายเป็นเจ้าของการให้บริการลูกค้า นาฬิกา SLA และการรับรองผล ไม่คัดลอกสถานะ incident ไปปิด ticket

## ขั้นตอนจำลอง

1. ผู้จัดการเลือก ticket ต้นทางที่มีสิทธิ์อ่าน ระบุผลกระทบ severity ผู้รับที่ eligible และวันอัปเดตถัดไป สร้าง incident ที่ยึด Company/Branch/Department เดียวกับต้นทาง
2. เจ้าของ incident ได้ Work/Inbox และวันนัดใน Calendar รับ investigation แล้วบันทึกความคืบหน้า เจ้าของ ticket ที่เชื่อมได้รับแจ้งให้ตามลูกค้าของตน
3. ผู้จัดการ link เคสอื่นใน scope เดียวกันด้วย version ปัจจุบัน; unlink ที่ผูกผิดต้องมีเหตุผลและยังเหลือ source อย่างน้อยหนึ่งเคส ประวัติ link/unlink/relink เก็บใน immutable events
4. เจ้าของเสนอ root cause และ mitigation evidence ผู้จัดการที่เป็นคนละคนกับทั้งเจ้าของปัจจุบันและผู้เสนอ mitigation รับตรวจจาก Work อนุมัติหรือส่งกลับให้สืบสวนต่อ การเปลี่ยนผู้รับไม่เปิดทางให้ผู้เสนออนุมัติตัวเอง
5. เมื่อ confirm incident เคสลูกค้ายังคงสถานะ/version และ SLA เดิม ผู้รับแต่ละเคสต้องติดตามและยืนยันผลลูกค้าผ่าน flow ticket แยกกัน
6. ผู้จัดการ reassign, cancel พร้อมเหตุผล หรือ reopen หลังปิดได้ ถ้าผู้รับถูกถอนสิทธิ์ Work แสดง blocked และไม่ยอมให้ผู้รับเดิมดำเนินการ; ไม่มี independent reviewer ต้องจัด coverage โดยผู้ดูแล

## หลักฐานและขอบเขต

- Owner service `src/server/support-incidents.ts`, API `/api/v1/holdings/{holdingSlug}/support/incidents`, UI `/support/incidents`; additive migration0219
- ทุกคำสั่งใช้ expectedVersion/idempotency/current role และ transaction SERIALIZABLE; incident events เปลี่ยนย้อนหลังไม่ได้
- ทดสอบสองลูกค้าที่สร้าง quotation→ticket ผ่าน owner commands จริง: link/unlink/relink→investigate→mitigate→reject→rework→independent close→reopen โดย ticket ทั้งสองไม่ถูกแก้แทน
- ทดสอบ Company/Branch/Department, stale/racing commands, duplicate links, inactive owner/reviewer และป้องกัน self-review หลัง reassignment
- รายการแสดงล่าสุดไม่เกิน200รายการ; ไม่มีการนับรายการต่าง scope

## งานที่ยังเหลือ

การเชื่อม incident ข้าม scope แผนก/สาขา, incident commander/ทีมผู้เชี่ยวชาญหลาย domain, incident-specific SLA/severity policy, public status/Portal/Email, attachments, postmortem และ customer acceptance ครบ scenario ยังเป็น implementation/acceptance gaps ไม่ใช่ external gate ทั้งหมด การทดลองนี้ไม่มีข้อความหรือแจ้งเตือนส่งออกให้ลูกค้าจริง
