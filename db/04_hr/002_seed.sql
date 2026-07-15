-- folio/db/04_hr/002_seed.sql
-- Seed 5 employees + 4 leave_requests translated from hr-line-agent/db/seed.sql.
-- Targets folio_db / hr schema. Idempotent: TRUNCATE first.

BEGIN;

TRUNCATE TABLE hr.leave_requests CASCADE;
TRUNCATE TABLE hr.employees CASCADE;

INSERT INTO hr.employees (id, employee_code, name, department, position, role, job_description, total_annual_leave, used_annual_leave, used_sick_leave) VALUES
('a1b2c3d4-e5f6-7a8b-9c0d-1e2f3a4b5c6d', 'EMP001', 'สมชาย ดีใจ', 'Engineering', 'Senior Developer', 'staff',
 'พัฒนาและดูแลระบบเว็บแอปพลิเคชัน เขียน unit test และทำ code review ร่วมกับทีมร่วมงาน', 10, 0, 1),
('b2c3d4e5-f67a-8b9c-0d1e-2f3a4b5c6d7e', 'EMP002', 'สมศรี รักงาน', 'Engineering', 'Engineering Manager', 'manager',
 'บริหารทีมวิศวกร วางแผนการทำงาน ตรวจสอบคุณภาพงาน และพิจารณาอนุมัติคำขอลาหยุดของคนในทีม', 12, 0, 0),
('c3d4e5f6-7a8b-9c0d-1e2f-3a4b5c6d7e8f', 'EMP003', 'สมรักษ์ พารวย', 'HR', 'HR Specialist', 'hr',
 'ดูแลสวัสดิการของพนักงาน วางแผนกิจกรรม สรรหาบุคลากร และอนุมัติใบลาภาพรวมของบริษัท', 15, 2, 0),
('d4e5f67a-8b9c-0d1e-2f3a-4b5c6d7e8f9a', 'EMP004', 'สมพงษ์ คล่องแคล่ว', 'HR', 'HR Assistant', 'staff',
 'ช่วยประสานงานเอกสารภายในทีม HR บันทึกประวัติพนักงาน และดูแลการประสานสิทธิ์ต่างๆ', 10, 0, 0),
('e5f67a8b-9c0d-1e2f-3a4b-5c6d7e8f9a0b', 'EMP005', 'วิภา พรประเสริฐ', 'HR', 'HR Director', 'hr',
 'กำกับดูแลนโยบายบริหารงานบุคคล พัฒนาองค์กร พิจารณาอนุมัติวันลาในระดับผู้บริหารสูงสุด', 20, 0, 0);

INSERT INTO hr.leave_requests (employee_id, leave_type, start_date, end_date, days, reason, status, approved_by, created_at) VALUES
('a1b2c3d4-e5f6-7a8b-9c0d-1e2f3a4b5c6d', 'annual', '2026-06-25', '2026-06-26', 2.0, 'พักผ่อนกับครอบครัวต่างจังหวัด', 'pending', NULL, NOW() - INTERVAL '1 hour'),
('d4e5f67a-8b9c-0d1e-2f3a-4b5c6d7e8f9a', 'sick', '2026-06-24', '2026-06-24', 1.0, 'เป็นไข้หวัดใหญ่ ปวดศีรษะสูง', 'pending', NULL, NOW() - INTERVAL '30 minutes'),
('c3d4e5f6-7a8b-9c0d-1e2f-3a4b5c6d7e8f', 'annual', '2026-06-12', '2026-06-13', 2.0, 'ทำธุระส่วนตัวเรื่องบ้านใหม่', 'approved', 'e5f67a8b-9c0d-1e2f-3a4b-5c6d7e8f9a0b', NOW() - INTERVAL '10 days'),
('a1b2c3d4-e5f6-7a8b-9c0d-1e2f3a4b5c6d', 'sick', '2026-06-10', '2026-06-10', 1.0, 'พบแพทย์รักษาฟันคุด', 'approved', 'c3d4e5f6-7a8b-9c0d-1e2f-3a4b5c6d7e8f', NOW() - INTERVAL '12 days');

COMMIT;
