# Talent lifecycle simulation

สถานะนี้เป็น **owner implementation บนข้อมูลจำลอง** สำหรับ S17/G16 ยังไม่ใช่ candidate portal, production หรือ business sign-off

## Flow ที่ส่งมอบ

1. Talent สร้าง requisition ตาม Branch/Department; ผู้สร้างอนุมัติเองไม่ได้ ก่อนเปิดรับสมัครต้องผ่าน reviewed version
2. candidate record เก็บ consent evidence และ retention date; เฉพาะผู้มีสิทธิ์ review จึงอ่านข้อมูลส่วนบุคคลได้
3. application ผูก requisition/candidate version และ current eligible owner; เจ้าของที่หมดสิทธิ์แสดง routing blocker และ reviewer reassign ได้โดยไม่สร้าง application ใหม่
4. เจ้าของ application บันทึก interview proposal พร้อม recommendation, score และ evidence; ผู้เสนอรับรอง proposal เองไม่ได้
5. offer สร้างได้หลัง accepted proposal และ selected application เท่านั้น ยอดเงินเก็บเป็น minor units และต้อง independent approval ก่อนส่ง
6. การยอมรับ offer ต้องมี acceptance evidence แล้วจึงสร้าง immutable onboarding handoff ที่ snapshot offer version และ organization scope

ทุก command ใช้ idempotency key และ optimistic version; event/source history กับ hire handoff ลบไม่ได้

## หลักฐานและ gate ที่เหลือ

- `tests/talent.test.ts` ตรวจ complete lifecycle, privacy, stale source, self-review, owner revocation/reassignment และ immutable handoff
- regression รวม People lifecycle, Work views และ migration replay: `docs/testing/evidence/2026-09-20-talent-tests.txt`
- ยังเหลือ candidate self-service/verified email, interview scheduling/Calendar, offer e-signature/background check, People onboarding consumer, retention erasure/export, browser หลายบทบาท, UAT และ production sign-off

