# Planning and manufacturing simulation

> Recorded report — findings, counts, gaps and hostname observations below belong to the original run. Current workflows: [owner catalogue](../contracts/next/catalogue.md); latest recorded engineering/provider/release boundaries: [verification inventory](../status/capability-delivery-matrix.md). This report is retained evidence, not an active plan.

สถานะนี้เป็น owner implementation บนข้อมูลจำลองสำหรับ S03/S11 และ G03/G10. Versioned BOM/formula เชื่อมกับ scenario ที่คำนวณ shared-stock shortage, capacity และ feasible date; recommendation ต้อง independent review ก่อนสร้าง order. Production ใช้ reservation กลาง, actual material issue/cost, labor/overhead, output/scrap, independent QC, finished stock, cost snapshot และ genealogy

หลักฐาน 15 tests ใน `docs/testing/evidence/2026-09-21-production-tests.txt` ครอบ production, Sales/Field shared reservation, Quality regression และ migration replay. ยังเหลือ incoming PO/sales-demand netting, detailed resource calendars, multi-operation/machine downtime, lot/serial/recall, WIP/GL posting, browser หลายบทบาทและ business acceptance
