# CRM leads และ opportunities บนข้อมูลจำลอง

ขอบเขต: ชุด D / S01 มี owner service, additive migration0215, API `/api/v1/holdings/{holdingSlug}/crm`, หน้า CRM, Work, Inbox และ Calendar. เป็น implementation บางส่วนของ capability ไม่ใช่ production sign-off.

## ผู้ใช้และการส่งต่อ

1. Sales สร้าง lead พร้อมผู้ติดต่อ แหล่งที่มา brief และวันติดตาม ผู้สร้างเป็นเจ้าของเริ่มต้นและต้องมีสิทธิ์/engagement ปัจจุบัน.
2. Sales manager เปลี่ยนผู้รับได้เมื่อระบุเหตุผล ผู้รับใหม่ต้อง eligible ใน Company/Branch/Department ของงาน. ผู้รับเห็น Work/Inbox/Calendar; เจ้าของเดิมไม่สามารถดำเนินการแทนหลังส่งต่อ.
3. ผู้รับ qualify โดยเลือก customer ที่มีอยู่และประมาณการมูลค่า/วันปิด หรือ disqualify พร้อมเหตุผล. Lead หนึ่งรายการมี opportunity เดียว. Manager reopen lead ที่ตัดออกได้.
4. สร้างและอนุมัติ quotation ผ่าน Sales workflow เดิม แล้ว link quotation ที่มี customer/scope/currency ตรงกัน. เก็บเวอร์ชันที่อนุมัติ; เอกสาร draft ใช้ไม่ได้. Brief ยังอยู่ต้นทางให้ผู้รับตรวจ ไม่ได้ถูกคัดลอกเป็นเงื่อนไขสัญญาโดยอัตโนมัติ.
5. Negotiation ต้องใช้ quotation ที่ยัง approved และเวอร์ชันไม่เปลี่ยน. Won ต้องมี sales order จาก quotation เดิม ผ่าน independent approval เดิมแล้ว; ไม่สร้าง sales order ปลายทางลัด workflow.
6. Lost เก็บเหตุผล; manager reopen พร้อมทบทวน quotation ใหม่ได้. ประวัติเก็บแบบแก้ทับ/ลบไม่ได้. การคืน/ยกเลิก order ภายหลังยังไม่เปิด deal ใหม่อัตโนมัติ.

## Forecast

แสดง net ก่อนภาษีตาม scope ปัจจุบัน แยก currency; discovery25% / proposal50% / negotiation75%. ปัด half-up ต่อรายการก่อนรวมด้วย BigInt และส่ง JSON เป็น decimal integer strings จึงไม่สูญเสียหน่วยย่อยเมื่อผลรวมเกิน safe Number. ใช้ currency contract สองตำแหน่งจาก Commerce ปัจจุบัน; ยังไม่ใช่ multi-currency precision engine. Won เป็น snapshot มูลค่าตอนปิด ไม่ใช่รายได้ GL และไม่ใช่ยอดสุทธิหลังคืนสินค้า.

## การควบคุม

Typed commands, current assignment/recipient checks, Idempotency-Key, lead/opportunity expectedVersion, transaction และ immutable events. คำสั่ง opportunity เพิ่ม version ของทั้ง lead และ opportunity เพื่อชนกับ reassignment อย่างถูกต้อง. Source quote/order เปลี่ยน customer หรือ scope หลังมี evidence ไม่ได้. ไม่มีการรับเงิน ตัด stock หรือส่งข้อความภายนอกจาก CRM.

## ยังเหลือก่อนรับรองครบ

- Customer identity/merge/authority/consent, lead deduplication และ field correction ที่มีหลักฐาน.
- Activity บน lead/opportunity โดยตรง, next-action history, business-calendar SLA/escalation และ routing repair queue กลาง.
- การส่ง brief/contract obligations ลง quotation/project อย่างครบถ้วน, forecast policy snapshots และ multi-currency precision.
- Portal/public projection, Email delivery, Marketing attribution และ customer acceptance.
- Browser หลายบทบาทและ UAT ทั้ง S01; automated ผ่านไม่ปลด gate เหล่านี้.

หลักฐาน automated เก็บแยกจาก browser และ production ตาม ledger. ทดสอบผ่าน normal owner commands รวม quote conversion handoff; ไม่แก้ fixture ด้วยการข้าม approval.
