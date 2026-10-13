# Agent governance simulation

> Recorded report — findings, counts, gaps and hostname observations below belong to the original run. Current workflows: [owner catalogue](../contracts/next/catalogue.md); latest recorded engineering/provider/release boundaries: [verification inventory](../status/capability-delivery-matrix.md). This report is retained evidence, not an active plan.

สถานะนี้เป็น **owner implementation บนข้อมูลจำลอง** สำหรับ S20/G18 ยังไม่ใช่ model evaluation, production หรือ business sign-off

Agent catalog และ version ต้องผ่าน independent approval; version snapshot prompt, source allowlist และข้อห้าม `approve/sign/pay` เปลี่ยนย้อนหลังไม่ได้. Version ที่อนุมัติแล้วจึง assign ให้ current user หรือ pool ตาม path scope ได้

Workspace Assistant เดิมยังเป็น read-only และสร้าง private session แยก company/user/assignment/authority scope/conversation. ทุก provider attempt บันทึก immutable invocation evidence ทั้ง success/failure ได้แก่ version ที่ route ได้, question/source digests, authority/provider snapshots, token usage และ error โดยไม่เก็บสิทธิ์ใหม่ให้ AI

หลักฐาน `tests/integration/agent/agent-governance.test.ts`, `tests/integration/agent/workspace-assistant.test.ts` และ migration replay รวม 9 tests อยู่ที่ `docs/testing/evidence/2026-09-21-agent-governance-tests.txt`. ยังเหลือ source adapters ทุก domain, evaluation/monitoring, pool balancing, retention/export/legal hold, browser หลายบทบาท, provider/production และ business acceptance
