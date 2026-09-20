# ยกเลิก quality hold ที่สร้างผิดก่อนตรวจ

เปิด Quality → hold → Request cancellation พร้อมเหตุผล. ระหว่างรออนุมัติ สินค้ายังถูกกัก ห้ามจองหรือส่งออก และการตรวจต้องรอให้ผู้จัดการตัดสินคำขอยกเลิกก่อน.

Quality manager ที่ไม่ใช่ผู้สร้าง hold หรือผู้ขอยกเลิก เลือก reject เพื่อกลับไปตรวจตามปกติ หรือ approve เพื่อยกเลิก hold นี้. เก็บทุกคำขอและคำตัดสินแบบ immutable พร้อม actor/version/reason. ไม่สร้างผล pass ปลอม ไม่ลบ hold และไม่เปลี่ยนจำนวน/มูลค่าสต็อกหรือ GL.

- เคยมี inspection แล้ว: ห้ามยกเลิก ทั้งผล pass และ fail ต้องดำเนิน disposition/release เดิม.
- มี hold อื่นซ้อนกัน: ยกเลิกหนึ่งรายการไม่ทำให้สินค้าพ้นกักทั้งหมด.
- ตรวจและขอยกเลิกพร้อมกัน: source version/transaction ยอมให้สำเร็จทางเดียว.
- ผู้ review หมดสิทธิ์: ปฏิเสธคำสั่งและคง quarantine; Work แสดงปัญหาการครอบคลุมสิทธิ์ตามกลไกเดิม.
- Work/Inbox/Calendar ใช้สถานะ source; cancellation ไม่ใช่ผลตรวจผ่าน.

Migration 0213 เพิ่มตาราง history และ fields และขยาย status constraint โดยไม่แก้สถานะรายการเก่า. Release ก่อนหน้าที่ไม่รู้จัก cancelled อาจยังแสดง hold ว่าค้าง; stock guards ในฐานข้อมูลใช้ชุดสถานะใหม่. ต้อง rollout application ที่รองรับ status ก่อนใช้ cancellation UI; ไม่มี destructive cleanup.

ขอบเขตยังเป็นทั้ง product/warehouse ไม่ใช่ lot/serial. Browser หลายบทบาทและ business acceptance ยังต้องรับรองแยก; automated simulation ไม่ใช่ production sign-off.
