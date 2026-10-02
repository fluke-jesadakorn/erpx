# Browser acceptance บนฐานข้อมูลจำลองแยก

ใช้ `bun --conditions=react-server scripts/local/browser-simulation.ts prepare` เพื่อสร้างฐานข้อมูลทดสอบหกชุดด้วย `erpx_test_admin` บน `127.0.0.1:5432` พร้อมสำเนาแอปและบัญชีจำลองหกบทบาท ไม่มี session ที่สร้างข้ามขั้น login และไม่มีธุรกรรมปลายทางที่ทำให้ scenario ผ่านล่วงหน้า

คำสั่งคืน run ID และตำแหน่งไฟล์ credentials ที่อ่านได้เฉพาะเจ้าของไฟล์ ห้ามนำ password หรือ runtime secret ลงรายงาน/Git จากนั้นใช้ `start <run-id>` เปิดสำเนาบน loopback `3102` ด้วย Node 24.20.0 และ Bun 1.2.17 คำสั่งตรวจ port ก่อนเริ่มและไม่หยุด process ที่ครอบครอง port อยู่

สำหรับ browser case ที่ส่งไฟล์ ให้เปิด local scanner และ worker อย่างชัดเจนเมื่อ environment มี ClamAV พร้อม: `ERPX_BROWSER_SIMULATION_ATTACHMENTS=local ERPX_BROWSER_SIMULATION_JOBS=1 bun run demo:start <run-id>`. โหมดนี้ส่งเข้า simulation เฉพาะ socket, signature directory และตัวแปลง preview ของ local dev ที่ตรวจว่าพร้อมก่อนเริ่ม พร้อมสร้าง jobs bundle ในสำเนาของ run นั้นเอง ไม่ใช้ worker ของ dev/staging และไม่ส่ง provider credentials เข้า simulation ค่าเริ่มต้นยังปิด scanner และ jobs worker ไว้

- `3100` และ public development คงเดิม; ไม่แตะ production `3101` หรือ Tunnel
- Environment ของ server เป็น allowlist; ไม่คัดลอก `.env` หรือ provider credentials
- ใช้ Source operator, Receiving operator, Finance reviewer, Quality inspector Quality release manager และ Purchasing manager ผ่านหน้า Sign in ปกติ สลับบัญชีผ่าน Sign out all ใน environment จำลอง
- เปิดงานจาก Work queue ของผู้รับเอง ตรวจทั้งสิ่งที่ทำได้และสิ่งที่ต้องรอคนอื่น ระบุ source IDs และยอดจริงที่หน้าจอแสดง
- สำเนาแอปเป็น snapshot ตอน prepare หากแก้โค้ดหลังจากนั้น ต้องบันทึกไฟล์ที่อัปเดตและ checksum ใหม่ก่อนอ้างว่า browser ตรวจ revision นั้นแล้ว
- การผ่าน flow บางส่วนเป็น observation เท่านั้น ไม่ใช่การผ่าน capability ทั้งรายการหรือ production sign-off

หลังเก็บหลักฐาน ใช้ `destroy <run-id>` ครั้งแรกหยุดเฉพาะ process group ที่ตรวจสอบ command path ตรงกับ run นั้น ใช้ซ้ำเมื่อ process ออกแล้วเพื่อลบฐานข้อมูลที่ชื่อและเจ้าของตรงกับ run และลบ directory ของ run นั้น ห้ามใช้ wildcard ลบฐานข้อมูลหรือหยุด service อื่น

บัญชีและ stock ยกมาเป็น fixture เท่านั้น ไม่มี email/payment/provider จริง การตรวจ live integrations, อุปกรณ์ และ business UAT ยังต้องแยกดำเนินการ

เวอร์ชันปัจจุบันเตรียม inventory/equity accounts แยกและ opening stock GL ผ่าน create/independent-post commands สำหรับการตรวจครั้งถัดไป หลักฐาน run เก่าระบุข้อจำกัดของผังบัญชีเดิมไว้ ไม่แก้ย้อนหลังหรืออ้างว่าตรวจ fixture ใหม่นี้แล้ว
