export type DemoActor = {
  label: string;
  email: string;
  userId: string;
  role: string;
};

export type GeneralCompanyDemoInput = {
  run: string;
  origin: string;
  holdingSlug: string;
  requestId: string;
  quotationId: string;
  organizationReviewer?: DemoActor;
  sourceWarehouseId?: string;
  targetWarehouseId?: string;
  movementProductId?: string;
  lotProductId: string;
  serialProductId: string;
  customerId?: string;
  customerDuplicateIds?: [string, string];
  customerOperator?: DemoActor;
  crmLeadId?: string;
  qualityDueSoonHoldId?: string;
  qualityOverdueHoldId?: string;
  qualityInspector?: DemoActor;
  qualityManager?: DemoActor;
  closePeriodIds: [string, string];
  controlledPaymentInvoiceId: string;
  actors: Record<
    | "employee"
    | "manager"
    | "hrApprover"
    | "payrollPreparer"
    | "buyer"
    | "purchasingManager"
    | "salesOperator"
    | "salesManager"
    | "warehouse"
    | "sourceWarehouse"
    | "financePreparer"
    | "financeApprover"
    | "treasury"
    | "administrator",
    DemoActor
  >;
};

export function generalCompanyDemoGuide(input: GeneralCompanyDemoInput) {
  const base = `${input.origin}/holdings/${input.holdingSlug}`;
  return {
    version: 1,
    environment: {
      kind: "isolated-real-workflow",
      run: input.run,
      origin: input.origin,
      persistence: "Dedicated disposable PostgreSQL databases and private local files",
      externalIntegrations: "Sandbox or unconfigured; no email, bank, payment, or provider action is live",
      controls: { supplierInvoiceMatching: "required" },
    },
    reset: {
      behavior: "Destroy this exact run, then prepare a fresh run from migrations and owner services",
      commands: [
        `bun run demo:destroy -- ${input.run}`,
        "bun run demo:prepare",
      ],
    },
    acceptance: {
      command: "bun run demo:verify",
      browserCommand: `ERPX_DEMO_RUN=${input.run} ERPX_E2E_BASE_URL=${input.origin} bun run demo:verify:browser`,
      test: "tests/integration/procurement/purchase-to-payment.test.ts",
      evidence: [
        "docs/testing/evidence/2026-09-21-multi-receipt-purchase-to-payment-tests.txt",
        "docs/testing/evidence/2026-09-21-multi-receipt-purchase-to-payment-context.json",
        "docs/testing/evidence/2026-09-21-supplier-invoice-policy-browser.txt",
        "docs/testing/evidence/2026-09-21-supplier-invoice-policy-browser-context.json",
      ],
      scope: "Owner-service automation plus isolated multi-role controlled-payment and two-period-close browser acceptance; provider, production and business gates remain separate",
    },
    scenarios: [
      {
        id: "general-company-laptop-purchase",
        title: "ขอซื้อ Laptop 10 เครื่องจนถึงการชำระเงิน",
        sourceRecord: { type: "employee_request", id: input.requestId },
        startingState: "Draft purchase request with fictional THB amounts and no completed downstream transaction",
        steps: [
          { order: 1, actor: "employee", action: "ตรวจรายละเอียดและส่งใบขอซื้อ", href: `${base}/requests/${input.requestId}`, expected: "คำขอเข้าสู่สายอนุมัติที่บันทึกเวอร์ชันไว้" },
          { order: 2, actor: "manager", action: "อนุมัติตามสายบังคับบัญชา", href: `${base}/work?type=purchase_request&view=actions`, expected: "ผู้ขออนุมัติคำขอตัวเองไม่ได้" },
          { order: 3, actor: "financePreparer", action: "ตรวจรายละเอียดทางบัญชี", href: `${base}/work?type=purchase_request&view=actions`, expected: "ขั้นตรวจบัญชีเสร็จโดยคนละคนกับผู้ขอ" },
          { order: 4, actor: "financeApprover", action: "อนุมัติวงเงินโดยอิสระ", href: `${base}/work?type=purchase_request&view=actions`, expected: "คำขอพร้อมส่งให้ฝ่ายจัดซื้อ" },
          { order: 5, actor: "buyer", action: "สร้าง RFQ และบันทึกข้อเสนอผู้ขาย", href: `${base}/procurement-rfqs`, expected: "ปริมาณสินค้าตรงกับ demand ที่อนุมัติ" },
          { order: 6, actor: "purchasingManager", action: "ตรวจการเปรียบเทียบและอนุมัติผู้ขาย", href: `${base}/work?type=procurement_rfq&view=actions`, expected: "สร้าง PO จากข้อเสนอที่อนุมัติเท่านั้น" },
          { order: 7, actor: "warehouse", action: "รับสินค้า 6 เครื่อง แล้วรับอีก 4 เครื่อง", href: `${base}/work?type=commerce_order&view=actions`, expected: "หน้าใบขอซื้อแสดง 6/10 แล้วเปลี่ยนเป็น 10/10 โดยไม่รับซ้ำ" },
          { order: 8, actor: "financePreparer", action: "เลือก receipt ทั้ง 6 และ 4 เครื่องเพื่อจับคู่ใบแจ้งหนี้ผู้ขายหนึ่งใบ แล้วเตรียม AP รวมหนึ่งใบ", href: `${base}/supplier-bills`, expected: "ใบแจ้งหนี้เชื่อม receipt ทั้งสอง และ quantity/price variance ต้องได้รับการทบทวนก่อนลงบัญชี" },
          { order: 9, actor: "financeApprover", action: "อนุมัติและลงบัญชีเจ้าหนี้", href: `${base}/work?type=supplier_bill&view=actions`, expected: "GL และ outstanding payable เชื่อมกับหลักฐานต้นทาง" },
          { order: 10, actor: "treasury", action: "เตรียม ส่งออก และยืนยันผลการจ่ายจากหลักฐานธนาคาร", href: `${base}/finance/supplier-batches`, expected: "ยอดจ่ายและ bank match ถูกบันทึกครั้งเดียว" },
          { order: 11, actor: "employee", action: "กลับไปตรวจผลในใบขอซื้อ", href: `${base}/requests/${input.requestId}`, expected: "progress แสดงครบทุก owner stage โดยไม่คัดลอก authority record" },
        ],
        actors: input.actors,
      },
      {
        id: "split-requisition-and-unit-conversion",
        title: "แบ่งคำขอซื้อให้ผู้ขายหลายรายและแปลงหน่วยนับ",
        sourceRecord: { type: "employee_request", id: input.requestId },
        startingState: "Reset the demo first, then use the same approved ten-laptop request as an alternative sourcing exercise",
        steps: [
          { order: 1, actor: "employee", action: "ส่งคำขอ Laptop 10 เครื่อง โดยใช้หน่วย each", href: `${base}/requests/${input.requestId}`, expected: "คำขอเก็บจำนวนและหน่วยต้นทางเป็นหลักฐาน" },
          { order: 2, actor: "manager", action: "อนุมัติคำขอตามสายอนุมัติ", href: `${base}/work?type=purchase_request&view=actions`, expected: "คำขอพร้อมให้ฝ่ายจัดซื้อจัดสรร" },
          { order: 3, actor: "buyer", action: "สร้าง RFQ แรก 6 each และ RFQ ที่สอง 4 each; ทดลองเลือกสินค้าที่ซื้อเป็น case พร้อมระบุอัตราแปลง", href: `${base}/procurement-rfqs?request=${input.requestId}`, expected: "หน้าจอแสดง requested, allocated และ remaining; จัดสรรรวมเกิน 10 ไม่ได้" },
          { order: 4, actor: "purchasingManager", action: "อนุมัติ supplier award ของแต่ละ RFQ แยกกัน", href: `${base}/work?type=procurement_rfq&view=actions`, expected: "แต่ละ award สร้าง PO ของผู้ขายรายนั้นและอ้างอิงรายการคำขอเดียวกัน" },
          { order: 5, actor: "buyer", action: "เปิดคำขอและตรวจประวัติ PO หลายใบ", href: `${base}/requests/${input.requestId}`, expected: "progress รวมทุก PO และยังเก็บ conversion evidence ของแต่ละ RFQ" },
        ],
        actors: input.actors,
      },
      {
        id: "finance-two-consecutive-closes",
        title: "กระทบยอดและปิดบัญชีสองงวดต่อเนื่อง",
        sourceRecord: { type: "finance_period", id: input.closePeriodIds[0] },
        startingState: "January and February 2027 contain a posted opening journal, matching customer/supplier schedules, and exact period-end sandbox bank statements",
        steps: [
          { order: 1, actor: "financePreparer", action: "เปิด January และตรวจ GL, receivable, payable และ bank difference", href: `${base}/finance/close`, expected: "ทุก control account แสดง difference 0 และ statement ลงวันที่ 2027-01-31" },
          { order: 2, actor: "financePreparer", action: "เริ่มปิด January", href: `${base}/finance/close`, expected: "ระบบเก็บ reconciliation snapshot และเปลี่ยนสถานะเป็น closing" },
          { order: 3, actor: "financeApprover", action: "ตรวจหลักฐานและอนุมัติ January", href: `${base}/finance/close`, expected: "ผู้เริ่มปิดอนุมัติเองไม่ได้ และ January เปลี่ยนเป็น closed" },
          { order: 4, actor: "financePreparer", action: "ตรวจ carry-forward และเริ่มปิด February", href: `${base}/finance/close`, expected: "ยอดสะสมตรงกับ statement วันที่ 2027-02-28 และสร้าง snapshot คนละ close version" },
          { order: 5, actor: "financeApprover", action: "อนุมัติ February พร้อมหลักฐาน", href: `${base}/finance/close`, expected: "ปิดบัญชีต่อเนื่องครบสองงวดโดย AR/AP/bank difference เป็นศูนย์" },
        ],
        actors: { financePreparer: input.actors.financePreparer, financeApprover: input.actors.financeApprover },
      },
      {
        id: "sales-to-collection",
        title: "ใบเสนอราคาจนถึงส่งสินค้า ออกใบแจ้งหนี้ และรับชำระ",
        sourceRecord: { type: "commerce_quote", id: input.quotationId },
        startingState: "A fictional customer quotation and available stock are ready in the isolated demo company",
        steps: [
          { order: 1, actor: "salesOperator", action: "ตรวจและส่งใบเสนอราคาให้อนุมัติ", href: `${base}/commerce?view=quote`, expected: "ใบเสนอราคาถูกส่งพร้อม version และผู้รับผิดชอบ" },
          { order: 2, actor: "salesManager", action: "อนุมัติใบเสนอราคาโดยอิสระ", href: `${base}/work?type=commerce_order&view=actions`, expected: "ผู้จัดทำอนุมัติเอกสารของตัวเองไม่ได้" },
          { order: 3, actor: "salesOperator", action: "แปลงใบเสนอราคาเป็น sales order แล้วส่งคำสั่งซื้อ", href: `${base}/commerce?view=sales`, expected: "sales order อ้างอิงใบเสนอราคาเดิมและรักษาราคา snapshot" },
          { order: 4, actor: "salesManager", action: "อนุมัติ sales order", href: `${base}/work?type=commerce_order&view=actions`, expected: "คลังเห็นงานส่งมอบพร้อมเจ้าของและกำหนดส่ง" },
          { order: 5, actor: "warehouse", action: "บันทึกหลักฐานส่งมอบสินค้าครบจำนวน", href: `${base}/commerce?view=sales`, expected: "stock movement GL ถูกสร้างและสินค้าคงเหลือถูกหักตามจำนวนจริง" },
          { order: 6, actor: "financeApprover", action: "ตรวจและลงบัญชี stock movement", href: `${base}/work?type=journal&view=actions`, expected: "GL สินค้าโพสต์ก่อนออก invoice" },
          { order: 7, actor: "financePreparer", action: "เตรียม customer invoice", href: `${base}/commerce?view=sales`, expected: "invoice ผูกกับ fulfillment เดียวกันและเข้า queue ตรวจบัญชี" },
          { order: 8, actor: "financeApprover", action: "อนุมัติและโพสต์ customer invoice", href: `${base}/finance/invoices`, expected: "ลูกหนี้และรายได้เชื่อมกับ sales order และยอด outstanding ตรงกัน" },
          { order: 9, actor: "treasury", action: "บันทึกรับชำระจากลูกค้า", href: `${base}/finance/invoices`, expected: "invoice เป็น paid และ payment allocation เหลือ outstanding ศูนย์" },
        ],
        actors: { salesOperator: input.actors.salesOperator, salesManager: input.actors.salesManager, warehouse: input.actors.warehouse, financePreparer: input.actors.financePreparer, financeApprover: input.actors.financeApprover, treasury: input.actors.treasury },
      },
      {
        id: "finance-controlled-bank-payment",
        title: "จ่ายเจ้าหนี้ด้วยการควบคุมสามบทบาทและหลักฐานธนาคาร",
        sourceRecord: { type: "finance_invoice", id: input.controlledPaymentInvoiceId },
        startingState: "A fictional THB 125 supplier invoice is posted, March 2027 is open, and one exact outgoing sandbox bank row is waiting to be matched",
        steps: [
          { order: 1, actor: "financePreparer", action: "เปิดใบแจ้งหนี้และเตรียม controlled payment", href: `${base}/finance/invoices/${input.controlledPaymentInvoiceId}`, expected: "สร้าง payment draft ที่ผูก supplier, amount และ bank control account" },
          { order: 2, actor: "financeApprover", action: "ตรวจและอนุมัติ payment พร้อมหลักฐาน", href: `${base}/finance`, expected: "ผู้เตรียมอนุมัติรายการตัวเองไม่ได้" },
          { order: 3, actor: "treasury", action: "release payment แล้วเลือก exact sandbox bank row เพื่อ confirm", href: `${base}/finance/bank`, expected: "ระบบ post GL, allocate invoice, settle payment และ match bank row ใน transaction เดียว" },
          { order: 4, actor: "treasury", action: "reload payment และเปิด posted journal", href: `${base}/finance`, expected: "สถานะ settled, outstanding เป็นศูนย์ และหลักฐานยังอยู่หลัง reload" },
        ],
        actors: { financePreparer: input.actors.financePreparer, financeApprover: input.actors.financeApprover, treasury: input.actors.treasury },
      },
      {
        id: "traceable-receipt-and-quality-release",
        title: "รับสินค้าตาม Lot/Serial และกักกันเพื่อรอตรวจ",
        sourceRecord: { type: "commerce_product", id: input.lotProductId },
        startingState: "Lot-tracked medical supplies and serial-tracked service devices are active, with no stock balance until a real approved purchase receipt is recorded",
        steps: [
          { order: 1, actor: "buyer", action: "สร้างและส่ง PO สำหรับสินค้าที่ติดตามด้วย lot หรือ serial", href: `${base}/commerce?view=purchase&new=1`, expected: "ใบสั่งซื้อแสดงวิธีติดตามของสินค้าที่เลือก" },
          { order: 2, actor: "purchasingManager", action: "อนุมัติ PO โดยอิสระ", href: `${base}/work?type=commerce_order&view=actions`, expected: "คลังได้รับ next action สำหรับการรับสินค้า" },
          { order: 3, actor: "warehouse", action: "บันทึก lot/serial แยกรายการ และเลือกหนึ่ง lot เป็น Quarantine พร้อมเหตุผลและวันครบกำหนด", href: `${base}/commerce?view=purchase`, expected: "ยอด on hand เพิ่ม แต่ยอด quarantined ไม่รวมใน available และสร้างงาน Quality อัตโนมัติ" },
          { order: 4, actor: "financePreparer", action: "ตรวจ stock movement และ GL จาก receipt เดียวกัน", href: `${base}/finance/ledger`, expected: "มูลค่ารวมของ lot ตรงกับ stock control total และ draft journal" },
          { order: 5, actor: "warehouse", action: "ตรวจตำแหน่งสต็อก Available, Reserved, Quarantined และ In transit", href: `${base}/commerce?view=stock`, expected: "สถานะแต่ละประเภทแสดงแยกกันโดยไม่รวมซ้ำ" },
          { order: 6, actor: "administrator", action: "เปิดคิว Quality เพื่อตรวจหลักฐาน lot/serial ที่กักกัน", href: `${base}/quality`, expected: "การตรวจและการปล่อยต้องเป็นคนละคนกับผู้สร้าง hold" },
        ],
        products: { lotTracked: input.lotProductId, serialTracked: input.serialProductId },
        actors: input.actors,
      },
      {
        id: "temporary-task-delegation",
        title: "มอบหมายงานแทนชั่วคราว ถอนสิทธิ์ และตรวจรับงาน",
        startingState: "Four independent demo colleagues are active; no task exists until the administrator creates it in the real Work interface.",
        steps: [
          { order: 1, actor: "administrator", action: "สร้างงานตรวจรับ Laptop ให้ employee และเลือก completion approver เป็น financeApprover จากนั้นส่งงาน", href: `${base}/work?new=task`, expected: "งานเป็น ready และผู้ตรวจรับเป็นคนละคนกับผู้สร้างและผู้รับผิดชอบ" },
          { order: 2, actor: "administrator", action: "ใน Temporary delegation เลือก manager เป็นผู้ทำแทน กำหนดวันหมดอายุและเหตุผล", href: `${base}/staff-tasks`, expected: "หน้ารายละเอียดแสดงผู้รับงานแทน วันหมดอายุ และเหตุผลหลัง reload" },
          { order: 3, actor: "manager", action: "เปิดงานที่ได้รับมอบหมาย เริ่มทำและบันทึกความคืบหน้า", href: `${base}/work`, expected: "บันทึกการตรวจรับบางส่วนไว้ให้เจ้าของงานรับช่วงต่อ" },
          { order: 4, actor: "administrator", action: "ถอนการมอบหมายพร้อมระบุเหตุผล แล้วเปิด Delegation history", href: `${base}/staff-tasks`, expected: "เก็บผู้ถอน เวลา และเหตุผล; ผู้ทำแทนที่ไม่มีสิทธิ์อื่นอ่านงานนี้ต่อไม่ได้" },
          { order: 5, actor: "employee", action: "ตรวจบันทึกเดิม ทำงานส่วนที่เหลือ และส่ง completion", href: `${base}/work`, expected: "ผู้รับผิดชอบเดิมรับงานต่อได้พร้อมหลักฐานการถอนสิทธิ์" },
          { order: 6, actor: "financeApprover", action: "ตรวจและอนุมัติ completion โดยอิสระ", href: `${base}/work`, expected: "งาน completed และประวัติยังอยู่หลัง reload" },
        ],
        actors: { administrator: input.actors.administrator, employee: input.actors.employee, manager: input.actors.manager, financeApprover: input.actors.financeApprover },
      },
      ...(input.organizationReviewer ? [{
        id: "reviewed-organization-setup",
        title: "ตั้งสาขาและแผนกผ่านผู้ตรวจอิสระ พร้อมประวัติการเปลี่ยนแปลง",
        startingState: "An administrator and a separate Company HR reviewer are active. Proposed units are not available for staff placement before approval.",
        steps: [
          { order: 1, actor: "administrator", action: "เปิด Propose a new branch or department เลือก Branch แล้ว Save draft", href: `${base}/people/organization`, expected: "Draft อยู่ในคิวและเปิดแก้ไขต่อได้หลัง reload โดยยังไม่สร้างสาขา" },
          { order: 2, actor: "administrator", action: "เปิด Correct and resubmit แล้ว Submit for review", href: `${base}/people/organization`, expected: "สถานะ awaiting review และผู้ขออนุมัติเองไม่ได้" },
          { order: 3, actor: "reviewer", action: "เปิด Work → Organization changes เลือกคำขอ แล้วส่งกลับพร้อมเหตุผลให้ปรับชื่อหรือขอบเขตสาขา", href: `${base}/work?type=organization_change&view=actions`, expected: "ผู้ขอได้รับแจ้งเตือนพร้อมลิงก์กลับคำขอเดิม" },
          { order: 4, actor: "administrator", action: "เปิด Inbox → Updates แล้วกดคำขอที่ส่งกลับ แก้ไขและส่งใหม่", href: `${base}/inbox?tab=updates`, expected: "ลิงก์เปิดตรงคำขอแม้สถานะเปลี่ยน เก็บประวัติ draft, submit และ rejection ครบ" },
          { order: 5, actor: "reviewer", action: "Approve and apply จากนั้นตรวจ Change status = Applied", href: `${base}/people/organization`, expected: "สร้างสาขาและช่องสนทนาองค์กรพร้อมกัน; Decision history ระบุผู้ตัดสินใจและเหตุผล" },
          { order: 6, actor: "administrator", action: "เสนอแผนกใต้สาขาใหม่ และให้ reviewer อนุมัติ", href: `${base}/people/organization`, expected: "แผนกอยู่ใต้สาขาที่เลือกเท่านั้น" },
          { order: 7, actor: "reviewer", action: "ทดลองอนุมัติการระงับสาขาที่ยังมีแผนก active", href: `${base}/people/organization`, expected: "ระบบแสดง dependency ที่ต้องย้ายหรือปิดก่อน และสาขายัง active" },
          { order: 8, actor: "administrator", action: "ยกเลิกคำขอที่ติด dependency แล้วเสนอระงับแผนกก่อนสาขา ให้ reviewer ตรวจแต่ละขั้น และทดลอง reactivate สาขาก่อนแผนก", href: `${base}/people/organization`, expected: "การระงับและเปิดใหม่เก็บ revision และประวัติครบ ค้นหาได้ด้วยตัวกรอง All requests" },
          { order: 9, actor: "administrator", action: "สร้างสาขาปลายทางและแผนกใหม่ จากนั้นเพิ่ม position ที่ใช้งานอยู่ใต้แผนกต้นทาง", href: `${base}/people/organization?view=positions`, expected: "ทั้งสาขา แผนก และ position ผ่าน Company HR review และ position อ้างอิงแผนกต้นทาง" },
          { order: 10, actor: "administrator", action: "เสนอ relocate แผนกไปสาขาปลายทาง แล้วให้ reviewer อนุมัติ", href: `${base}/people/organization`, expected: "เก็บแผนกเดิมเป็น archived history, สร้าง node ที่สาขาใหม่ และย้าย position ที่ใช้งานอยู่" },
          { order: 11, actor: "administrator", action: "archive แผนกว่าง แล้วเสนอ reactivate โดยให้ reviewer คนเดิมตรวจแยกแต่ละคำขอ", href: `${base}/people/organization`, expected: "node เดิมและ decision history ยังอยู่ และสถานะกลับเป็น active หลังอนุมัติ" },
        ],
        actors: { administrator: input.actors.administrator, reviewer: input.organizationReviewer },
      }, {
        id: "company-setup-acceptance",
        title: "ตรวจความพร้อม Company และรับรองโดยผู้ดูแลอีกคน",
        startingState: "The isolated Company has an active branch, six staff levels, one CEO position and placement for every active staff member. Acceptance has not been submitted.",
        steps: [
          { order: 1, actor: "administrator", action: "เปิด Business readiness ตรวจ checklist แล้ว Submit for independent review พร้อมเหตุผล", href: `${base}/settings/business-readiness#company-setup-acceptance`, expected: "สถานะ Awaiting independent review และผู้ส่งไม่เห็นปุ่มอนุมัติ" },
          { order: 2, actor: "reviewer", action: "เปิด Work → Company setup acceptance แล้ว Return for correction พร้อมเหตุผล", href: `${base}/work?type=company_business_acceptance&view=actions`, expected: "ผู้ส่งได้รับ Inbox update พร้อมลิงก์ตรงกลับหน้ารับรอง" },
          { order: 3, actor: "administrator", action: "เปิด Inbox update ตรวจหลักฐานและส่งใหม่", href: `${base}/inbox?tab=updates`, expected: "สถานะ Awaiting independent review และประวัติ rejection ยังคงอยู่" },
          { order: 4, actor: "reviewer", action: "กลับไปหน้า Business readiness แล้ว Accept Company setup", href: `${base}/settings/business-readiness#company-setup-acceptance`, expected: "สถานะ Accepted พร้อมเหตุผลและผู้ตรวจอิสระ" },
          { order: 5, actor: "administrator", action: "Revoke acceptance พร้อมเหตุผล แล้วเปิด Immutable acceptance history", href: `${base}/settings/business-readiness#company-setup-acceptance`, expected: "สถานะ Revoked และ event ทั้งห้ายังอยู่หลัง reload" },
        ],
        actors: { administrator: input.actors.administrator, reviewer: input.organizationReviewer },
      }] : []),
      ...(input.sourceWarehouseId && input.targetWarehouseId && input.movementProductId && input.actors.sourceWarehouse
        ? [{
            id: "inventory-staged-transfer-and-partial-receipt",
            title: "โอนสต็อกระหว่างคลัง รับของบางส่วน และปิดยอดขนส่ง",
            sourceRecord: { type: "inventory_transfer", id: `demo-transit-${input.run}` },
            startingState: "Demo Company has 3 fictional SIM-ITEM units in the source warehouse and an empty receiving warehouse; the scenario uses disposable isolated storage.",
            steps: [
              { order: 1, actor: "sourceWarehouse", action: "สร้าง staged warehouse transfer จำนวน 3 หน่วย พร้อมวันถึงและเหตุผล", href: `${base}/stock-transfers?new=1`, expected: "เอกสาร draft เก็บ source, target, quantity และมูลค่าต้นทุนไว้" },
              { order: 2, actor: "sourceWarehouse", action: "ส่ง transfer ให้ผู้ตรวจอิสระ", href: `${base}/stock-transfers`, expected: "เอกสารเปลี่ยนเป็น submitted และผู้จัดทำอนุมัติเองไม่ได้" },
              { order: 3, actor: "warehouse", action: "ตรวจเส้นทางและอนุมัติ transport", href: `${base}/work?type=transfer&view=actions`, expected: "เอกสารเปลี่ยนเป็น approved for dispatch โดยมี decision evidence" },
              { order: 4, actor: "sourceWarehouse", action: "บันทึก dispatch tracking และหลักฐาน carrier", href: `${base}/stock-transit`, expected: "สต็อกออกจาก source ไปอยู่ใน in-transit โดยยังคงมูลค่าเดิม" },
              { order: 5, actor: "warehouse", action: "รับเข้าปลายทาง 1 หน่วย แล้ว reload และรับอีก 2 หน่วย", href: `${base}/stock-transit`, expected: "partial receipt แสดง 1/3 และเมื่อจบแสดง completed, received 3, lost 0 พร้อม stock ปลายทาง 3 หน่วย" },
            ],
            warehouses: { source: input.sourceWarehouseId, target: input.targetWarehouseId },
            productId: input.movementProductId,
            actors: { sourceWarehouse: input.actors.sourceWarehouse, warehouse: input.actors.warehouse },
          }]
        : []),
      ...(input.qualityDueSoonHoldId && input.qualityOverdueHoldId && input.qualityInspector && input.qualityManager
        ? [{
            id: "quality-escalation-recovery",
            title: "รับงาน Quality ที่ใกล้ครบกำหนดและเกินกำหนด พร้อมหลักฐานการกู้คืน",
            sourceRecord: { type: "quality_alerts", id: input.qualityDueSoonHoldId },
            startingState: "The isolated Demo company contains one active hold due today and another hold overdue by one day. Both are routed to independent Quality recipients and can be reset with the Demo run.",
            steps: [
              { order: 1, actor: "warehouse", action: "เปิด Work และตรวจ Quality escalations ของ hold ที่ใกล้ครบกำหนดและเกินกำหนด", href: `${base}/work?type=quality_alert`, expected: "แสดง due-soon และ overdue แยกกัน พร้อมวันครบกำหนด ผู้รับผิดชอบ และ next action" },
              { order: 2, actor: "qualityInspector", action: "เปิด due-soon alert และ acknowledge พร้อมระบุผู้ตรวจและเวลาที่จะตรวจ", href: `${base}/quality/${input.qualityDueSoonHoldId}`, expected: "การรับงานถูกบันทึกครั้งเดียวหลัง reload และ alert เปลี่ยนเป็น acknowledged โดยไม่สร้าง notification ซ้ำ" },
              { order: 3, actor: "qualityInspector", action: "เปิด overdue alert และ acknowledge พร้อมเหตุผลการกู้คืนที่ล่าช้า", href: `${base}/quality/${input.qualityOverdueHoldId}`, expected: "ระบบเก็บเหตุผลและผู้รับผิดชอบของ overdue escalation; retry ด้วย idempotency เดิมไม่ทำซ้ำ" },
              { order: 4, actor: "qualityManager", action: "ตรวจ queue เดียวกันและยืนยันว่า alert ที่ถูก acknowledge แล้วมีแผน follow-up ต่อ", href: `${base}/quality`, expected: "ผู้จัดการเห็น source hold เดิมและสถานะ acknowledged แต่ไม่สามารถแก้หลักฐานของอีกคนโดยไม่ผ่าน version guard" },
              { order: 5, actor: "administrator", action: "ถอนสิทธิ์ Quality inspector แล้ว reload Work", href: `${base}/work?type=quality_alert`, expected: "สิทธิ์ใหม่มีผลทันที; alert ที่ยังไม่รับงานจะไม่แสดงเป็น actionable และระบบบอกให้ administrator แต่งตั้งผู้รับใหม่" },
            ],
            actors: { warehouse: input.actors.warehouse, qualityInspector: input.qualityInspector, qualityManager: input.qualityManager, administrator: input.actors.administrator },
          }]
          : []),
      ...(input.qualityDueSoonHoldId && input.qualityInspector && input.qualityManager
        ? [{
            id: "quality-inspection-and-release",
            title: "ตรวจ Quality แบบอิสระและปล่อย Stock หลังผ่านการตรวจ",
            sourceRecord: { type: "quality_hold", id: input.qualityDueSoonHoldId },
            startingState: "The isolated Demo company contains an active hold created by Warehouse. The hold is ready for an independent Quality inspector and a separate Quality manager release.",
            steps: [
              { order: 1, actor: "qualityInspector", action: "เปิด hold และบันทึกผล inspection เป็น Pass พร้อม evidence", href: `${base}/quality/${input.qualityDueSoonHoldId}`, expected: "สถานะเปลี่ยนเป็น awaiting_release และเก็บ inspector, version และหลักฐานแบบ immutable" },
              { order: 2, actor: "qualityManager", action: "เปิด hold เดิม ตรวจผล Pass และอนุมัติปล่อย", href: `${base}/quality/${input.qualityDueSoonHoldId}`, expected: "ผู้ตรวจไม่สามารถปล่อย stock เอง; manager อีกคนต้องให้เหตุผลและใช้ version ล่าสุด" },
              { order: 3, actor: "warehouse", action: "เปิด Stock register หลัง reload", href: `${base}/commerce?view=stock`, expected: "สถานะ quarantined ถูกตัดออกจาก available ตาม hold ที่ปล่อยแล้ว และประวัติ Quality ยังเชื่อมกับ lot หรือ product เดิม" },
            ],
            actors: { warehouse: input.actors.warehouse, qualityInspector: input.qualityInspector, qualityManager: input.qualityManager },
          }]
          : []),
      ...(input.customerId && input.customerOperator
        ? [{
            id: "customer-360-scoped-timeline",
            title: "Customer 360 แสดงประวัติที่เชื่อมกันตามสิทธิ์",
            sourceRecord: { type: "finance_party", id: input.customerId },
            startingState: "The isolated Demo customer has a real quotation owned by Sales. The timeline projection is read-only, paginated, and keeps each source record authoritative.",
            steps: [
              { order: 1, actor: "customerOperator", action: "เปิด Customer 360 จากรายการลูกค้า", href: `${base}/customers/${input.customerId}`, expected: "เห็น quotation/order activity พร้อม status, version และเวลาที่เกิดเหตุการณ์" },
              { order: 2, actor: "customerOperator", action: "เปิด source quotation จาก timeline", href: `${base}/customers/${input.customerId}`, expected: "ลิงก์พาไป source record เดิมเพื่อทำ action; timeline ไม่สร้างสำเนาเอกสาร" },
              { order: 3, actor: "salesManager", action: "เปิด timeline ด้วย role อื่นที่มี Commerce read", href: `${base}/customers/${input.customerId}`, expected: "เห็นรายการที่อนุญาตเหมือนกัน แต่ไม่เห็นข้อมูลติดต่อ ต้นทุน หรือข้อมูลของ Finance/Legal ที่ role นี้ไม่มีสิทธิ์" },
              { order: 4, actor: "customerOperator", action: "ใช้ Older activity เมื่อมี cursor และกลับ Latest activity", href: `${base}/customers/${input.customerId}`, expected: "pagination ใช้ cursor จาก server และไม่ข้าม scope หรือทำให้รายการซ้ำ" },
            ],
            actors: { customerOperator: input.customerOperator, salesManager: input.actors.salesManager },
          }]
        : []),
      ...(input.customerDuplicateIds
        ? [{
            id: "customer-duplicate-review-and-merge",
            title: "ตรวจคู่ลูกค้าซ้ำและรวมเข้าระเบียนหลัก",
            sourceRecord: { type: "finance_party", id: input.customerDuplicateIds[0] },
            startingState: "Two fictional active customer masters have similar legal names and the same simulated tax registration. A Company administrator requests review; a separate Sales manager reviews the identity evidence and selects the canonical record.",
            steps: [
              { order: 1, actor: "administrator", action: "ส่งคู่ลูกค้าที่อาจซ้ำพร้อมเหตุผลและหลักฐาน", href: `${base}/customer-duplicates`, expected: "ระบบเรียงคู่ให้คงที่และส่งงานให้ผู้จัดการบริษัทคนอื่น" },
              { order: 2, actor: "salesManager", action: "เปรียบเทียบชื่อ รหัส เลขทะเบียนภาษี และหลักฐาน", href: `${base}/customer-duplicates`, expected: "ผู้ตรวจเห็น identity fields ทั้งสองฝั่งและเลือก canonical ได้" },
              { order: 3, actor: "salesManager", action: "ยืนยัน duplicate และเลือก canonical customer", href: `${base}/customer-duplicates`, expected: "ประวัติ immutable ระบุผู้ตรวจ เหตุผล และ canonical ID" },
              { order: 4, actor: "salesManager", action: "รวม source records ที่ปลอดภัยเข้าระเบียนหลัก", href: `${base}/customer-duplicates`, expected: "คู่ review เป็น merged และ losing master คงเป็น blocked lineage" },
            ],
            actors: { administrator: input.actors.administrator, salesManager: input.actors.salesManager },
          }]
        : []),
      ...(input.crmLeadId && input.customerOperator
        ? [{
            id: "crm-lead-activities",
            title: "CRM มอบหมายกิจกรรมและบันทึกผลการติดต่อลูกค้า",
            sourceRecord: { type: "crm_lead", id: input.crmLeadId },
            startingState: "The isolated Demo Company contains an open CRM lead owned by Sales with no scheduled activity.",
            steps: [
              { order: 1, actor: "customerOperator", action: "เปิด lead และตรวจรายละเอียด customer needs", href: `${base}/crm/${input.crmLeadId}`, expected: "เห็น owner, due date และส่วน Lead activities ที่ยังว่าง" },
              { order: 2, actor: "customerOperator", action: "นัดหมาย email activity ให้ Sales owner พร้อมวันครบกำหนด", href: `${base}/crm/${input.crmLeadId}#crm-activities`, expected: "activity ถูกบันทึกด้วย lead version, owner และ history เดียวกันหลัง reload" },
              { order: 3, actor: "salesManager", action: "เปิด lead ด้วย manager role และ reassign activity เมื่อจำเป็น", href: `${base}/crm/${input.crmLeadId}#crm-activities`, expected: "manager เห็น activity และการเปลี่ยน owner ต้องมีเหตุผลและ version ใหม่" },
              { order: 4, actor: "customerOperator", action: "เปิด Work และบันทึก outcome ของกิจกรรม", href: `${base}/work?type=crm_activity&view=actions`, expected: "กิจกรรมออกจาก My actions หลัง complete และเก็บ outcome แบบ immutable" },
            ],
            actors: { customerOperator: input.customerOperator, salesManager: input.actors.salesManager },
          }]
        : []),
      {
        id: "marketing-consent-to-crm-lead",
        title: "ส่ง lead ที่ยินยอมจาก Marketing ให้ Sales ติดตาม",
        startingState: "A separate Sales manager can prepare a marketing form; an independent Company administrator reviews it. A fictional prospect is created only during this isolated exercise.",
        steps: [
          { order: 1, actor: "salesManager", action: "สร้าง Marketing form ที่ระบุช่อง contact/need และ consent requirement", href: `${base}/marketing`, expected: "แบบฟอร์มยังเป็น draft และใช้รับ lead ไม่ได้ก่อนผู้ดูแลอีกคนเปิดใช้" },
          { order: 2, actor: "administrator", action: "ตรวจ preview และ consent fields แล้ว Activate form", href: `${base}/marketing`, expected: "Form active ด้วย reviewer แยกจากผู้สร้าง" },
          { order: 3, actor: "administrator", action: "บันทึก prospect สมมติพร้อม contact, consent evidence, campaign, source และ brief", href: `${base}/marketing`, expected: "Lead candidate และ form_submit event ถูกบันทึกพร้อม attribution" },
          { order: 4, actor: "salesManager", action: "เปิด candidate บันทึกการตรวจ brief แล้ว Create CRM lead", href: `${base}/marketing`, expected: "Candidate เชื่อม CRM lead ที่มี brief, attribution, due date และ Sales manager เป็น owner" },
          { order: 5, actor: "salesManager", action: "เปิด CRM lead จาก Marketing candidate", href: `${base}/crm`, expected: "source record ยังคง authoritative และ lead เปิดต่อไปยังกิจกรรม Sales ได้" },
        ],
        actors: { administrator: input.actors.administrator, salesManager: input.actors.salesManager },
      },
      {
        id: "employee-overtime-to-payroll",
        title: "พนักงานส่งชั่วโมงล่วงเวลาให้ผู้จัดการและ HR ตรวจ ก่อนเข้า Payroll preview",
        startingState: "The resettable Demo Company has an active employee engagement, an independent reporting manager, an independent HR reviewer, and a THB pay group. No overtime request exists until the employee creates it.",
        steps: [
          { order: 1, actor: "employee", action: "เปิด Overtime เลือก Retrospective claim กรอกช่วงเวลาจริง เหตุผล และคำอธิบายการส่งย้อนหลัง ตรวจชั่วโมง แล้วบันทึก draft ก่อน submit", href: `${base}/overtime/new`, expected: "Draft อยู่ต่อหลัง reload และ submit เข้าคิว Manager verification พร้อมผู้ตรวจสองขั้นที่ระบุชัด" },
          { order: 2, actor: "manager", action: "เปิด Overtime request ที่มอบหมาย ตรวจช่วงเวลาและหลักฐาน แล้ว Verify actual hours", href: `${base}/overtime`, expected: "ผู้จัดการคนเดิมที่ทำคำขอไม่ได้ และการอนุมัติส่งต่อไป HR โดยไม่ข้ามขั้น" },
          { order: 3, actor: "hrApprover", action: "เปิดคำขอเดิม ตรวจประวัติ manager แล้ว Approve hours & hand off", href: `${base}/overtime`, expected: "สถานะ Approved และมีข้อความยืนยันการส่งชั่วโมงไป payroll preview หลัง reload" },
          { order: 4, actor: "payrollPreparer", action: "สร้าง payroll preview ครอบคลุมวันทำงาน แล้วตรวจ employee calculated item และ source handoff", href: `${base}/people?view=payroll`, expected: "Input snapshot ระบุ request ID และนาทีที่อนุมัติ; global payroll preview ไม่อ้างว่าเป็น statutory tax calculation" },
        ],
        actors: { employee: input.actors.employee, manager: input.actors.manager, hrApprover: input.actors.hrApprover, payrollPreparer: input.actors.payrollPreparer },
      },
    ],
  } as const;
}
