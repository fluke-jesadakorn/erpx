# ชุดจ่ายเจ้าหนี้ — การจำลอง S02 / ช่วง 1

สถานะ: implement flow จ่ายเจ้าหนี้ที่ผูก invoice และทดสอบอัตโนมัติแล้วบางส่วนของ capability; ยังไม่ใช่ production sign-off หรือการปิดช่วง 1 ทั้งหมด

## ขอบเขตและผู้รับช่วง

| ขั้น | ผู้รับผิดชอบ | สิ่งที่ต้องตรวจ / ผลลัพธ์ |
| --- | --- | --- |
| เตรียมชุดจ่าย | Company Finance preparer (`finance.write`) | เลือก posted supplier invoices, ยอดไม่เกินยอดว่าง, วันจ่าย, ชื่อและบัญชีผู้รับ; กันยอดโดยไม่ตัดหนี้ |
| อนุมัติ | คนอื่นที่มี `finance.approve` | ตรวจ source invoice, ยอด, บัญชีผู้รับที่บันทึกในชุดจ่าย และเก็บเหตุผล; ผู้เตรียมอนุมัติเองไม่ได้ |
| Export | Treasury คนที่สาม (`finance.pay`) | CSV เป็นทะเบียนคำสั่งจ่ายด้วยมือ พร้อม SHA-256 และ receipt ที่เรียกคืนได้; ยังไม่มี cash journal |
| ยืนยันผล | Treasury | เลือก bank transaction ที่นำเข้าและตรวจสอบแล้ว บัญชี/สกุลเงิน/จำนวน/ทิศทาง/วันต้องตรง; post payment, allocation, invoice balance, bank match และ GL ใน transaction เดียว |
| ไม่ทราบผล | Treasury | บันทึก unknown และหลักฐาน; ยังคงกันยอดเพื่อป้องกันจ่ายซ้ำ |
| ยืนยันไม่สำเร็จ | Finance approver คนอื่นจากผู้ export | บันทึกหลักฐานธนาคารว่าไม่ตัดเงินแล้วจึงปล่อยยอด; เตรียมชุดใหม่ได้เฉพาะยอดที่เหลือ |
| ยกเลิกก่อน export | ผู้เตรียมหรือ Finance approver | เก็บเหตุผลและปล่อย reservation; ถ้า export แล้วต้องตรวจผลธนาคารรายรายการ |

ทุกบทบาทต้องเป็น assignment ปัจจุบันระดับ Company; Branch/Department ไม่เห็นข้อมูลชุดจ่าย บัญชีผู้รับ จำนวนคิว หรือ notification ของ Company ผ่าน feature นี้ ผู้ใช้เข้าจาก Finance → Supplier payment batches, Work, Inbox หรือ Calendar layer Supplier payments โดย owner service เป็นผู้กำหนดสถานะ

## ยอดและบัญชี

ตัวอย่าง invoice A 100, invoice B 50: ชุดจ่ายเลือก A 60 + B 50 → outstanding ยังเป็น 100/50 จนกว่าจะมีหลักฐานธนาคาร ถ้า A สำเร็จจะบันทึก Dr เจ้าหนี้ 60 / Cr ธนาคาร 60, outstanding A เหลือ 40 และ B ยัง 50 เมื่อ B เป็น unknown ยอด 50 ยังถูกกัน หากผู้ตรวจอิสระยืนยัน failure จึงปล่อย B ให้เตรียมใหม่

- ใช้ credit account และ chart จาก invoice ต้นทาง; ไม่สร้างหนี้อีกชุด
- ชุดจ่ายไม่แก้ beneficiary/ยอดหลังอนุมัติ ต้องยกเลิกก่อน export และเตรียมใหม่
- การชำระ/allocate เดิมและการ approve/complete supplier return ถูก guard ระหว่างมี reservation; ป้องกันตัด invoice เดียวกันสองช่องทาง
- Guard การเตรียมกับ contractor batch เดิมใน People ทั้งสองทาง; ถ้ามี legacy contractor batch ที่ไม่ cancelled จะไม่สร้างชุดใหม่จาก invoice เดิม เพราะยังไม่ยืนยันการกระทบยอดของเส้นทางเก่า
- การเริ่มและ final close งวดถูก guard หากยังมี exported/unknown supplier instruction ที่ planned date ไม่เกินวันสิ้นงวด
- คำสั่งใช้ idempotency, source/version guard และตรวจ assignment ใน transaction; การเตรียมชุดพร้อมกันไม่ทำให้จองเกินยอด

## ข้อจำกัดที่ต้องทำต่อ

- ใช้ invoice แบบ standalone / commerce fulfillment, หน่วยเงินสองตำแหน่ง, สกุลเงิน functional currency และบัญชีธนาคารที่ Company กำหนดใน Commerce เท่านั้น; ยังไม่รองรับ FX หรือการเลือกหลายธนาคาร
- CSV เป็นทะเบียนสำหรับเจ้าหน้าที่ ไม่ใช่ bank-specific upload และไม่มี Bank API
- Payroll batches, return refund instructions และ supplier batches ยังเป็น owner workflow แยกกัน; ยังไม่ถือว่าปิด shared payment obligation/batch interface ทั้งระบบ
- ยังไม่มี supplier beneficiary master approval, due-date business-calendar/SLA escalation และการทำ compensating correction หลังยืนยันจ่ายสำเร็จ
- ไม่แก้ยอด/approval/export เดิมย้อนหลัง การแก้ legacy batch ต้องตรวจหลักฐานจริงโดยเจ้าของงาน
- หน้า list แสดง 200 รายการล่าสุด; invoice selector 200 รายการตาม due date; Work ไม่ใช้ limit นี้
- ยังไม่ผ่านผู้รับรองธุรกิจ, การจ่ายเงินจริง หรือ UAT หลายบทบาทผ่าน browser

## หลักฐาน

- `tests/supplier-payments.test.ts` ล่าสุด 4 tests ผ่าน รวม failed → เตรียมใหม่ → ชำระครบ → Work completed: multi-invoice partial settlement, independent approval/payer, replay/source mismatch, concurrent reservation, cancellation boundary, uncertain/failure review, bank mismatch/stale version, closed period, revoked role, Work/Inbox/Calendar scope และการชนกับ contractor batch เดิม
- `tests/commerce-returns.test.ts`: reservation กัน supplier return จนยกเลิกชุดที่ยังไม่ export แล้ว return ดำเนินต่อได้
- Regression 7 suites ที่เกี่ยวข้อง: 30 tests passed ก่อนเพิ่มกรณีล่าสุด; owner tests หลังเพิ่ม guard: 7 passed; ชุดล่าสุด supplier/payroll/work cache/refresh: 20 passed
- Browser บน public dev ใช้ session holding owner เดิม: หน้า list/form แสดง posted invoice, สกุลเงินและบัญชีที่ configure ถูกต้อง, checkbox เปิดช่องกรอกตามรายการ, Calendar layer เปิดได้ ไม่มีการ submit ชุดจ่ายจาก Acme ในการตรวจนี้
- Durable test summary: [supplier cross-owner tests](evidence/2026-09-20-supplier-tests.txt); เป็น observation แบบ partial ใน acceptance ledger ไม่ยกระดับ gate ทั้ง capability

รายการ failed ที่ invoice ยังมียอดค้างยังปรากฏใน Work ให้ Finance ตามต่อ แม้ instruction ในชุดเดิมจะไม่มี unknown/exported เหลือแล้ว; เมื่อชำระต้นทางครบจึงจบคิวนี้

## วิธีปฏิบัติเมื่อเกิดปัญหา

1. Response export หาย: ใช้ Retrieve original export ตรวจ SHA-256 เดิม ห้ามเตรียมชุดใหม่เพราะไม่เห็น response
2. ไม่รู้ว่าธนาคารตัดเงินหรือไม่: บันทึก unknown ติดต่อผู้ตรวจธนาคารและคงยอดที่กันไว้
3. ธนาคารยืนยันไม่จ่าย: Finance คนอื่นจากผู้ export บันทึก failed พร้อมหลักฐาน แล้วเตรียม instruction ใหม่สำหรับยอดที่ยังค้าง
4. ธนาคารจ่ายแล้วแต่ post ไม่สำเร็จ: ห้ามจ่ายซ้ำ ใช้ bank transaction เดิมตรวจ currency/account/period/version แล้ว retry โดยไม่แก้ประวัติการจ่ายภายนอก
5. งวดปิด: ให้ผู้มีอำนาจจัดการ reopen ตาม policy; ระบบไม่ย้ายวันบัญชีให้เอง
