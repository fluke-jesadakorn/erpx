# Marketing lifecycle simulation

สถานะนี้เป็น owner implementation บนข้อมูลจำลองสำหรับ S01/S16 และ G01/G15. Form, content, domain และ provider readiness ต้องผ่าน independent activation; preview ไม่ถูกนับเป็น active. Lead candidate snapshot form version, consent, attribution และ brief; engagement เป็น immutable evidence. Reviewer ส่งต่อเข้า `createCRMLead` ของ CRM จริงด้วย idempotency แทนการคัดลอกปลายทางเอง

หลักฐาน 14 tests ใน `docs/testing/evidence/2026-09-21-marketing-tests.txt` ครอบ owner flow, CRM regression, Work views และ migration replay. ยังเหลือ public hosting, email/provider webhooks, bounce/unsubscribe, campaign analytics, browser หลายบทบาท, provider/production และ business acceptance

