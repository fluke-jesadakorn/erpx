# Folio — YouTube Series Script

> Seven episodes, one persona per shot. Each block is a self-contained filming
> package: bilingual narration, click-path demo steps, on-screen overlays, and
> the psql/curl evidence a viewer can re-run.
>
> Companion doc: `docs/youtube-casts.md` — cast, departments, cull rules.
> Tile catalog source of truth: `db/perm/*.sql`.

## Conventions

- **Language**: Thai voiceover, English subtitles. Bilingual on-screen overlays
  where the UI is already bilingual (most buttons, all tile names).
- **Persona badge** (top-left corner of every shot):
  `🪪 <name> · <dept> · L<n>` — matches `docs/youtube-casts.md`.
- **Tile badge** (bottom-right): `<tile-id> · <href>` — matches
  `db/perm/*.sql`.
- **Evidence footer** — every episode ends with copy-paste psql/curl one-liners
  so the viewer can verify the screenshot in 30 seconds.
- **Out of scope**: marketing copy, music cues, color grading. Ops only.

## Stack quick-reference

```
Browser → web-admin (Next.js :3003, single Next.js process)
       └→ lib/* via tsconfig path alias @folio-lib/*
            ├→ Postgres  (folio_db, owner contract)
            ├→ MinIO     (bucket folio-storage, key YYYY/MM/<uuid>)
            └→ Ollama    (bge-m3 for embeddings, qwen2.5/3.6 for chat,
                          vision models for slip OCR)
```

Single-process stack (Fastify `rbac-svc`/`ai-svc` services were
collapsed into `lib/{perm,ai,slips,finance}/*` and consumed by Next.js
via `@folio-lib/*` tsconfig path aliases — see `app/tsconfig.json`).

---

## EP 01 — Attach Receipt + AI

> "พนักงานถ่ายรูปสลิป → AI ดึงยอดเงินอัตโนมัติ" /
> "Snap a receipt — AI fills the form."

| Field          | Value                                                       |
|----------------|-------------------------------------------------------------|
| Persona        | 🪪 John Staff · Development · L5 (default for `staff` role)  |
| Tile           | `submit-expense` · `/submit-expense` · group `workflow`     |
| Sub-view       | `submit`                                                    |
| AI section     | `staff:ocr` (task=`vision`)                                 |
| Stack path     | browser → `POST /api/upload` → ai-svc → MinIO → Ollama Vision + Swift N-API |
| Tables touched | `slips`, `expenses`                                         |
| Length target  | ~3 min                                                      |

### Setup

1. Sign in as **John Staff** (top-right persona menu).
2. Open `/submit-expense` directly, or click the **🧾 Submit Slip** tile on the hub.
3. Have a sample receipt ready — PDF or PNG, ≤ 10 MB. Use the bundled
   `samples/receipt-sample.pdf` for the recorded shot.

### Demo steps

1. Drag the receipt file onto the dashed upload zone
   (`app/src/components/SlipUpload.tsx:92`).
2. Watch the progress bar climb 0 → 100% via `xhr.upload.onprogress`.
3. The image preview thumbnail appears (PNG/JPG only).
4. Behind the curtain: file is `multipart/form-data` POSTed to
   `/api/upload` → forwarded to ai-svc → MinIO bucket `folio-storage`
   (`lib/slips/storage.ts`).
5. OCR pipeline fires three passes:
   - **Pass 1**: Ollama Vision (qwen2-vl) for line items.
   - **Pass 2**: Ollama bge-m3 embeddings for vendor fuzzy match.
   - **Pass 3**: Swift N-API macOS Vision binding at
     `lib/native/vision-ocr/build/Release/vision_ocr.node`.
6. Auto-populated fields appear in the expense form:
   `vendor_name`, `transaction_date`, `subtotal`, `vat_amount`,
   `total_amount`, `payment_method`.
7. Toggle the **Mock mode** checkbox (`SlipUpload.tsx:84`) to compare the
   side-by-side: mock preset vs real OCR. Same form fields, different
   upstream.

### Narration

🇹🇭
> "พนักงานแค่ลากไฟล์สลิปเข้ามาในกรอบ — ระบบจะอัปโหลดไป MinIO แล้วส่งให้
> Ollama Vision อ่านภาพ ผ่าน Swift N-API ของ macOS Vision เพื่อดึงข้อความ
> ทั้งหมดอีกชั้น จากนั้น AI จะเติมยอดเงิน วันที่ ชื่อร้าน และ VAT
> ลงในฟอร์มให้อัตโนมัติ ไม่ต้องพิมพ์เอง"

🇬🇧
> "Staff just drag the receipt into the dashed box. The system streams it
> to MinIO, then Ollama Vision reads the image and the macOS Swift N-API
> Vision binding extracts the text. AI fills the amount, date, vendor, and
> VAT into the form — no typing required."

### On-screen overlays

- **Title card** (0:00–0:05): `EP 01 · Attach Receipt + AI`
- **Lower-third** (0:10): `John Staff · Development · L5`
- **Tile badge** (persistent): `submit-expense · /submit-expense`
- **Callout** during step 5: `3-pass OCR: Ollama Vision → bge-m3 → Swift N-API`
- **Tooltips** over each auto-filled field (1.5s pop, then fade):
  `🪄 Extracted by AI · 92% confidence`

### Expected evidence

```bash
# 1. Row in slips with MinIO key + OCR confidence
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -c "
  SELECT id, expense_id, file_path, mime_type, file_size,
         ocr_confidence, ai_reasoning, uploaded_by, uploaded_at
  FROM slips
  WHERE uploaded_by = 1
  ORDER BY uploaded_at DESC LIMIT 1;"

# 2. Linked expense draft — fields auto-populated
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -c "
  SELECT id, submitter_id, vendor_name, transaction_date,
         subtotal, vat_amount, total_amount, status
  FROM expenses
  WHERE submitter_id = 1
  ORDER BY id DESC LIMIT 1;"

# 3. MinIO object exists (key from step 1 file_path)
mc ls local/folio-storage/$(date +%Y/%-m)/

# 4. AI invocation audit (OCR task, vision model)
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -c "
  SELECT actor_id, section_key, task, model_id,
         prompt_tokens, completion_tokens, latency_ms, created_at
  FROM ai_invocations
  WHERE section_key = 'staff:ocr'
  ORDER BY created_at DESC LIMIT 3;"
```

---

## EP 02 — Approve Step + AI Recommended

> "บันไดอนุมัติ 4 ขั้น + AI แนะนำ COA" /
> "Four-stage approval ladder with AI-suggested COA."

| Field          | Value                                                       |
|----------------|-------------------------------------------------------------|
| Personas       | 🪪 John → Andrew → Emily → Charles (the full chain)         |
| Tiles          | `submit-expense` → `approve-expense` → `review-queue` → `search-coa` |
| Stages walked  | `supervisor_review` → `head_review` → `cfo_review` → `ceo_review` |
| AI sections    | `hod:approve`, `am:review`, `acct:queue`, `acct:coa-search`  |
| Tables touched | `expenses`, `approval_logs`, `chart_of_accounts` (vector)    |
| Length target  | ~6 min                                                      |

### Stage ladder source of truth

`lib/perm/stages.ts:131` maps each `StageName` to its permission id:

```ts
submission               → 'stage:submission:act::allow'
dept_verification        → 'stage:dept_verification:act::allow'
dept_authorization       → 'stage:dept_authorization:act::allow'
accounting_verification  → 'stage:accounting_verification:act::allow'
accounting_supervision   → 'stage:accounting_supervision:act::allow'
accounting_authorization → 'stage:accounting_authorization:act::allow'
disbursement_authorization → 'stage:disbursement_authorization:act::allow'
cfo_authorization        → 'stage:cfo_authorization:act::allow'
ceo_authorization        → 'stage:ceo_authorization:act::allow'
awaiting_disbursement    → (terminal — no act perm)
disbursed                → (terminal — no act perm)
rejected                 → (terminal — no act perm)
```

Granting a persona `stage:<key>:act::allow` (via `perm.role_permissions`)
on a role makes that persona eligible to act on a waybill that is
currently parked at that stage. Resolution is role-level MIN level —
the lowest-numbered level wins, mirroring finance hierarchy.

The matrix decides who can act on which stage — same matrix as EP 03.

### Demo steps

1. **John submits** at `/submit-expense`. Workflow stepper shows
   `supervisor_review` highlighted (`app/src/components/WorkflowStepper.tsx`).
2. **Switch persona → Andrew** (Supervisor, Finance & Account, L4).
3. Open `/approve-expense`. The current stage is `supervisor_review`.
4. Click **Approve**. The expense moves to `head_review`.
   - Behind the curtain: `approval_logs` row written with `actor_id=27`,
     `from_stage='supervisor_review'`, `to_stage='head_review'`.
5. **Switch persona → Emily** (Manager, Finance & Account, L3).
6. Open `/review-queue`. The "AI-suggested COA" column shows the top
   `chart_of_accounts.code` candidate based on bge-m3 cosine similarity.
7. Click **🔍 Why this COA?** — the AI panel
   (`components/ai/AiActionButton.tsx`, section `acct:queue`) explains in
   one paragraph why the suggested account matches the receipt.
8. Approve → expense moves to `cfo_review`.
9. **Switch persona → Charles** (Executive, L1). `/approve-expense`.
10. CFO-narrative panel (`components/ai/ExecutiveNarrative.tsx`) shows
    today's risk line for this expense; Charles clicks **Approve**.
11. Final status: `approved`. Notification bell fires for John
    (`components/NotificationBellClient.tsx`).

### Narration

🇹🇭
> "หลังจาก John กดส่ง ระบบจะล็อกขั้นตอนปัจจุบันไว้ที่ supervisor_review
> Andrew หัวหน้างานเห็นรายการใน /approve-expense กดอนุมัติได้ในคลิกเดียว
> พอถึง Emily เธอเห็น COA ที่ AI แนะนำจากการเปรียบเทียบเวกเตอร์กับผังบัญชี
> และอธิบายเหตุผลสั้นๆ ว่าทำไมถึงเลือกบัญชีนี้
> สุดท้าย Charles ซีอีโอเห็นภาพรวมความเสี่ยงของวันนี้ก่อนกดอนุมัติปิดท้าย"

🇬🇧
> "Once John submits, the system locks the current stage at
> `supervisor_review`. Andrew sees the item in `/approve-expense` and
> approves in one click. When it reaches Emily, she sees the AI-suggested
> COA from vector similarity over the chart of accounts, with a short
> explanation of why that account fits. Finally, Charles the CEO sees
> today's risk line before signing off."

### On-screen overlays

- **Title card**: `EP 02 · Approve Step + AI Recommended`
- **Workflow stepper** has its current stage highlighted in cyan, completed
  stages in emerald, future stages in slate.
- **Callout** during step 6: `🤖 COA from bge-m3 cosine · top-1: 510200`
- **Callout** during step 10: `CFO narrative · 2 ย่อหน้า · Thai`
- **Counter** in lower-left: `Approval #<n> · <latency>ms`

### Expected evidence

```bash
# 1. The full approval chain for the just-approved expense
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -c "
  SELECT al.id, al.expense_id, al.actor_id, u.fullname, r.name AS role,
         al.from_stage, al.to_stage, al.note, al.created_at
  FROM approval_logs al
  JOIN users u ON u.id = al.actor_id
  JOIN roles r ON r.id = u.role_id
  WHERE al.expense_id = <id>
  ORDER BY al.id;"

# 2. Expense status walk
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -c "
  SELECT id, status, total_amount, updated_at
  FROM expenses
  WHERE id = <id>;"

# 3. The COA AI suggested (top-1 by embedding distance)
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -c "
  SELECT code, name_th, name,
         embedding <=> (SELECT embedding FROM chart_of_accounts
                        WHERE code = '510200') AS distance
  FROM chart_of_accounts
  ORDER BY embedding <=> (SELECT embedding FROM chart_of_accounts
                          WHERE code = '510200')
  LIMIT 5;"

# 4. AI invocations across this episode
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -c "
  SELECT section_key, task, model_id, latency_ms
  FROM ai_invocations
  WHERE created_at > NOW() - INTERVAL '10 minutes'
  ORDER BY created_at;"
```

---

## EP 03 — Permissions Matrix

> "แก้ cell เดียว — กระทบทั้งเมทริกซ์" /
> "Flip one cell, the whole matrix cascades."

| Field          | Value                                                       |
|----------------|-------------------------------------------------------------|
| Persona        | 🪪 Alex Admin · IT · L2 (default for `admin` role)          |
| Tile           | `permissions` · `/permissions` · group `hr` · sub_view `permissions` |
| Modules seen   | 28 tiles (`tile-*`)                                         |
| AI section     | `policy:editor` (lint a draft policy)                       |
| Tables touched | `perm.role_permissions`, `perm.user_roles`, `perm.audit`    |
| Length target  | ~5 min                                                      |

### Demo steps

1. **Sign in as Alex Admin.** Open `/permissions` (tile `permissions`).
2. The matrix is rendered as a sticky-header grid: rows = roles, columns =
   modules (`app/src/components/org-chart/PermStrip.tsx`).
3. Filter by group: tick **Workflow** → only `tile-submit-expense`,
   `tile-approve-expense`, `tile-my-history` remain.
4. Locate cell `(account_officer × tile-ledger)`. State is `deny` (red).
5. **Click** the cell → drawer opens with three actions: `allow`, `deny`,
   `inherit`. Pick **allow**.
6. The `BulkModal` (`components/org-chart/BulkModal.tsx`) asks for an audit
   reason — type *"Demo: allow ledger read for AP team"*.
7. Click **Apply** → `PATCH /api/cells` → cell turns emerald (allow).
8. The right-hand panel shows the cascade: downstream module
   `tile-reconciliation` and `tile-all-prs` also unlock for
   `account_officer` (group inheritance via the CASL ability built in
   `lib/perm/ability.ts`).
9. **Switch persona → Emily** (accounting_manager) → open her hub →
   reconciliation tile now visible (was hidden before).
10. **Switch back to Alex** → click **Export CSV** →
    `GET /api/export?format=csv` → file downloads with all cell changes
    annotated with the reason.
11. **AI lint pass**: open `/policy` (tile `policy`), paste a draft policy,
    click **🪄 Review Policy** (section `policy:editor`). The AI panel
    flags a contradiction with the cell flip from step 7.

### Narration

🇹🇭
> "เมทริกซ์ RBAC เป็นหัวใจของระบบ — แถวคือ role คอลัมน์คือ module
> แค่คลิกเปลี่ยน cell เดียว ระบบจะ cascade ผ่าน group inheritance ไปยัง tile
> ที่เกี่ยวข้องทั้งหมด การเปลี่ยนทุกครั้งถูกบันทึกใน audit log พร้อมเหตุผล
> และ export ออกเป็น CSV ได้ทันที ส่วนการเขียน policy ใหม่ AI จะช่วย
> ตรวจข้อขัดแย้งกับสิ่งที่เพิ่งแก้ในเมทริกซ์ให้อัตโนมัติ"

🇬🇧
> "The RBAC matrix is the heart of the system — rows are roles, columns
> are modules. One cell click cascades through group inheritance to every
> dependent tile. Every change is logged with a reason and exportable as
> CSV. When writing a new policy, AI lints it against the matrix changes
> you just made and flags contradictions."

### On-screen overlays

- **Title card**: `EP 03 · Permissions Matrix`
- **Cell color legend** (top-right, persistent): `🟢 allow · 🔴 deny · ⚪ inherit`
- **Cascade panel** (step 8): animated line connecting source cell →
  downstream tiles; tooltip on each shows the inherited rule.
- **Audit badge** (step 7): `Logged by Alex · reason captured`
- **AI panel** (step 11): section `policy:editor` + model chip.

### Expected evidence

```bash
# 1. The cell flip from step 5
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -c "
  SELECT rp.role_id, rp.permission_id, rp.granted_by, rp.granted_at
  FROM perm.role_permissions rp
  WHERE rp.role_id = 'account_officer::5'
    AND rp.permission_id = 'tile:ledger:view::allow'
  ORDER BY rp.granted_at DESC LIMIT 1;"

# 2. The audit entry (note the reason text)
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -c "
  SELECT actor, kind, target, occurred_at
  FROM perm.audit
  WHERE kind = 'role_permission.grant'
  ORDER BY occurred_at DESC LIMIT 1;"

# 3. The downstream cascade — what unlocked
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -c "
  SELECT split_part(permission_id, ':', 2) AS subject,
         COUNT(*) AS grant_count
  FROM perm.role_permissions
  WHERE role_id = 'account_officer::5'
    AND permission_id LIKE 'tile:%:view::allow'
  GROUP BY split_part(permission_id, ':', 2)
  ORDER BY grant_count DESC;"

# 4. The exported CSV — first 5 lines
curl -s -H "Cookie: folio_session=<alex-session>" \
  "http://localhost:3003/api/export?format=csv" | head -5
```

---

## EP 04 — Organization Chart

> "ลากคน — เปลี่ยนหัวหน้าได้ทันที" /
> "Drag a person — the reporting line updates live."

| Field          | Value                                                       |
|----------------|-------------------------------------------------------------|
| Persona        | 🪪 Alex Admin · IT · L2                                     |
| Tile           | `org-chart` · `/org-chart` · group `hr` · sub_view `org-chart` |
| Components     | `OrgChart`, `RoleNode`, `treeOps`, `Drawer`                 |
| Tables touched | `users`, `departments`, `users.reports_to_user_id`          |
| Length target  | ~4 min                                                      |

### Setup

1. Sign in as Alex. Open `/org-chart` (tile `org-chart`).
2. The tree loads from `GET /api/org` (route handler at
   `app/src/app/api/org/route.ts:4`) → `loadOrg()` in `lib/perm/auth.ts`
   (single-source-of-truth permission loader built atop the
   `lib/perm/grammar.ts` parser).

### Demo steps

1. Tree renders four department roots: **Development**, **Finance & Account**,
   **Executive**, **IT** (`docs/youtube-casts.md`).
2. **Pan/zoom** the chart (mouse wheel + drag canvas).
3. Click **John Staff** (id=1). The right-side drawer opens showing:
   employee_code, role, staff_level, reports_to_user_id, direct reports.
4. Click **Groups panel** tab → toggle visibility by dept.
5. **Drag John** from under Andrew and drop him under a different manager
   (e.g., a hypothetical Finance supervisor). Drop zone highlights emerald.
6. Drop fires `PATCH /api/roles/<john-id>/reparent`
   (`OrgChart.tsx:64`) with `{ new_parent_id, actor: 'ui-drag' }`.
7. Toast appears: *"John Staff now reports to <name>"*. The tree redraws.
8. Click **Compare** → pick two roles → side-by-side RBAC matrix diff
   (`org-chart/compare.ts`).
9. Click **Bulk modal** → bulk-allow `tile-approve-expense` for the entire
   Finance & Account group → `PATCH /api/cells` with multiple changes.
10. **Export** → `GET /api/export?format=json` (or `csv`) downloads the
    current tree with permission annotations.

### Narration

🇹🇭
> "แผนผังองค์กรแสดงโครงสร้าง 4 แผนก — Development, Finance & Account,
> Executive, IT แอดมินลากคนจากสายบังคับบัญชาเดิมไปยังหัวหน้าคนใหม่ได้
> ระบบจะเรียก PATCH /api/roles/{id}/reparent แล้ว refresh tree ทันที
> สามารถ compare role สองตัวแบบ side-by-side หรือ bulk-allow ทั้งกลุ่ม
> แล้ว export ออกเป็น JSON/CSV ได้ในคลิกเดียว"

🇬🇧
> "The org chart shows four departments — Development, Finance &
> Account, Executive, IT. Admins drag any person to a new reporting
> line; the system calls `PATCH /api/roles/{id}/reparent` and refreshes
> the tree instantly. Compare two roles side-by-side, bulk-allow a tile
> across a whole group, and export the annotated tree as JSON or CSV."

### On-screen overlays

- **Title card**: `EP 04 · Organization Chart`
- **Persona dots**: each node has a small L-badge (L1–L5) under the avatar,
  colour-coded by `staff_level` (L1 gold → L5 slate).
- **Drop zone** highlight during drag: emerald dashed border + "Drop here"
  label.
- **Toast** (step 7): 3-second slide-down, then fades.
- **Compare mode** (step 8): two columns, diff highlighted in rose.

### Expected evidence

```bash
# 1. The reparent — new reports_to_user_id
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -c "
  SELECT id, fullname, reports_to_user_id, staff_level
  FROM users
  WHERE id = 1;"

# 2. The audit trail
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -c "
  SELECT actor, kind, target, occurred_at
  FROM perm.audit
  WHERE kind = 'role.reparent'
  ORDER BY occurred_at DESC LIMIT 1;"

# 3. The tree as JSON (first 80 lines)
curl -s -H "Cookie: folio_session=<alex-session>" \
  http://localhost:3003/api/org | python3 -m json.tool | head -80

# 4. Bulk cells applied
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -c "
  SELECT COUNT(*) AS grant_count
  FROM perm.role_permissions rp
  JOIN perm.user_roles ur ON ur.role_id = rp.role_id
  JOIN perm.user_permissions dept ON dept.user_id = ur.user_id
                             AND dept.permission_id = 'user:dept:finance-2::allow'
  WHERE rp.permission_id = 'tile:expense:approve::allow';"
```

---

## EP 05 — Staff Level × Permissions Matrix

> "L1 เห็นทุกอย่าง — L5 เห็นแค่ของตัวเอง" /
> "Level controls the surface area."

| Field          | Value                                                       |
|----------------|-------------------------------------------------------------|
| Persona        | 🪪 Alex Admin (editing) · then John / Emily / Charles (viewing) |
| Tiles          | `permissions`, `directory`                                  |
| DB source      | `users.staff_level`, `roles.default_staff_level`            |
| Migration      | `db/apply_staff_level.sql`                                  |
| Tables touched | `users.staff_level`, role default backfill                  |
| Length target  | ~4 min                                                      |

### Level mapping (canonical)

From `db/apply_staff_level.sql:43` — single source of truth:

| Role                  | default_staff_level |
|-----------------------|---------------------|
| `ceo`                 | **1**               |
| `cfo`                 | 2                   |
| `admin`               | 2                   |
| `hr_manager`          | 3                   |
| `accounting_manager`  | 3                   |
| `head_of_department`  | 3                   |
| `manager`             | 3                   |
| `account_supervisor`  | 4                   |
| `supervisor`          | 4                   |
| `account_officer`     | **5**               |
| `accountant`          | 5                   |
| `hr`                  | 5                   |
| `it`                  | 5                   |
| `staff`               | 5                   |

`users.staff_level` is a per-user override; `NULL` falls back to
`roles.default_staff_level`.

### Demo steps

1. **Sign in as Alex Admin.** Open `/permissions`.
2. Add a new column **"Staff Level"** (filter chip at top, toggle on).
3. Sort roles by level: L1 at top (CEO), L5 at bottom (staff).
4. Highlight the gap: at L5 only `tile-submit-expense`, `tile-my-history`,
   `tile-my-prs` show as `allow`. Everything else: `deny` or `inherit`.
5. Open `/directory` (tile `directory`). Filter by level L5.
6. Pick **John Staff** (currently `staff_level=5`). Open his profile drawer.
7. Click **Edit** → change `staff_level` to **3** → save.
8. The hub redraws live (no reload): John's tile list now includes
   `tile-approve-expense` and `tile-review-queue` (group inheritance
   resolved client-side via `@folio-lib/perm/auth-client` →
   `components/tileAccess.ts`).
9. **Switch persona → John**. The new tiles are visible on his hub.
10. Roll back: as Alex, set John back to L5 → tiles disappear again.
11. Show the **canonical mapping** with `psql` and explain that `apply_staff_level.sql`
    is idempotent — re-run any time after a role rename.

### Narration

🇹🇭
> "ระดับ staff_level 1 ถึง 5 เป็นตัวควบคุมพื้นที่ที่ผู้ใช้เห็น — L1 คือซีอีโอ
> เห็นทุก tile, L5 คือ staff เห็นแค่ของตัวเอง roles.default_staff_level
> คือค่าดีฟอลต์ที่ผูกกับ role ส่วน users.staff_level เป็น override รายบุคคล
> แอดมินแค่เปลี่ยนตัวเลขเดียว ระบบจะ cascade tile list ใหม่ทันทีโดยไม่ต้อง reload"

🇬🇧
> "The staff level 1-5 is the surface-area dial — L1 (CEO) sees every
> tile, L5 (staff) sees only their own. `roles.default_staff_level` is
> the role default; `users.staff_level` is the per-user override. Change
> one number and the hub tile list cascades live, no reload required."

### On-screen overlays

- **Title card**: `EP 05 · Staff Level × Permissions Matrix`
- **Level ladder** (left margin, persistent during the whole episode):
  vertical bar 1→5 with role icons, colour-graded gold → slate.
- **Filter chip** "Staff Level" shown when active.
- **Before/After split** (step 8): John's hub before (3 tiles) on the left,
  after (7 tiles) on the right.

### Expected evidence

```bash
# 1. All six active personas and their levels
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -c "
  SELECT u.id, u.fullname, r.name AS role,
         u.staff_level, r.default_staff_level
  FROM users u
  JOIN roles r ON r.id = u.role_id
  WHERE u.id IN (1, 4, 15, 20, 27, 29)
  ORDER BY u.staff_level;"

# 2. Role-level defaults (level encoded inline as ::<n> in role_id)
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -c "
  SELECT id, display_name
  FROM perm.roles
  ORDER BY split_part(id, '::', 2)::int, id;"

# 3. John's tile list — what his effective permission set yields
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -c "
  SELECT t.id, t.display_name, t.group_name
  FROM perm.tiles t
  WHERE t.view_perm_id IN (
    SELECT permission_id FROM perm.user_permissions WHERE user_id = $1 AND revoked_at IS NULL
    UNION
    SELECT rp.permission_id FROM perm.role_permissions rp
      JOIN perm.user_roles ur ON ur.role_id = rp.role_id
     WHERE ur.user_id = $1
  )
  ORDER BY t.sort_order;"

# 4. Same query after level bump to 3 (compare count)
```

---

## EP 06 — Executive Summary AI (CFO / CEO)

> "CFO อ่าน 2 ย่อหน้า — CEO อ่าน 1 ย่อหน้า" /
> "Two audiences, two summaries, same KPIs."

| Field          | Value                                                       |
|----------------|-------------------------------------------------------------|
| Personas       | 🪪 Emily Manager (CFO, L3) · 🪪 Charles Executive (CEO, L1) |
| Tile           | `cockpit` · `/cockpit` · group `cockpit`                    |
| AI sections    | `cfo:cockpit` · `ceo:cockpit` · `ledger:commentary`         |
| Components     | `ExecutiveNarrative`, `CFOWorkspace`, `CEOWorkspace`        |
| Length target  | ~5 min                                                      |

### KPI snapshot (input to the AI)

Built in `ExecutiveNarrative.tsx:18`:

```
Cash position: <totalCash> THB
MTD expenses: <mtdExpenses> THB
Outstanding liabilities: <outstandingLiabilities> THB
Net cash flow (MTD): <netCashFlow> THB
Trial balance: <balanced|MISMATCH> (Dr <debit> / Cr <credit>)
```

### Demo steps — CFO wing

1. **Sign in as Emily.** Open `/cockpit` (tile `cockpit`).
2. The CFO workspace renders (`components/workspaces/CFOWorkspace.tsx`).
3. KPI cards line up across the top.
4. Locate the **📝 Executive Narrative** panel
   (`ExecutiveNarrative.tsx` with `audience="cfo"`).
5. Click **Generate executive narrative**.
6. Section key `cfo:cockpit` fires (`task="chat"`).
7. Behind the curtain:
   - System prompt (Thai, 2 paragraphs):
     *"Paragraph 1 covers cash and liquidity. Paragraph 2 covers expenses,
     liabilities, and pipeline. Thai, formal, no bullets, end each
     paragraph with a concrete number."*
   - Input: the KPI block above.
8. Result card appears within 1-3 s with the model name + latency chip
   (`AiActionButton.tsx:131`).
9. Click **Copy** → paste into clipboard.
10. Switch to GL commentary: click **🪄 Explain variance** on a ledger
    line → section `ledger:commentary`.

### Demo steps — CEO wing

1. **Switch persona → Charles** (Executive, L1). Open `/cockpit`.
2. The CEO workspace renders (`CEOWorkspace.tsx`).
3. Locate the **📊 Board Summary** panel (`audience="ceo"`).
4. Click **Generate board summary**.
5. Section key `ceo:cockpit` fires. System prompt (English, 1 paragraph):
   *"High-level, formal, forward-looking. Mention cash, expenses, and
   pipeline in plain numbers. End with one sentence on the top strategic
   risk."*
6. Result appears with model chip + latency.
7. Show the AI invocation drawer (right-side panel): `section_key`,
   `actor_id=15`, `prompt_tokens`, `completion_tokens`, `cost_usd`.
8. Compare tone: CFO paragraph is dense, two concrete numbers per
   paragraph; CEO paragraph is single, ends with a risk sentence.

### Demo steps — projections + loss/growth

1. From the CEO workspace, click **📈 Generate projections** → AI section
   `ledger:commentary` (reused) with a different system prompt that
   emphasises month-end forecast.
2. Result includes three scenarios: base, optimistic, downside.
3. Click **📉 Loss analysis** → AI re-explains the largest variance line
   in plain language with year-over-year context.

### Narration

🇹🇭
> "Cockpit เป็นห้องควบคุมทางการเงิน — ทุกเช้า Emily CFO เปิดดูตัวเลขสรุป
> แล้วกดปุ่มเดียวให้ AI เขียนบทสรุป 2 ย่อหน้าเป็นภาษาไทยทางการ
> ส่วน Charles CEO เปิด cockpit ของเขาเอง ได้บทสรุป 1 ย่อหน้าภาษาอังกฤษ
> ที่จบด้วยประโยคความเสี่ยงเชิงกลยุทธ์ ข้อมูลตัวเลขชุดเดียวกัน แต่ persona
> ของ AI ต่างกันตาม section key"

🇬🇧
> "The cockpit is the financial control room — every morning CFO Emily
> reviews the KPI snapshot and clicks once to get a 2-paragraph formal
> Thai narrative. CEO Charles opens his own cockpit and gets a single
> English paragraph ending with the top strategic risk. Same numbers,
> different AI persona keyed by section."

### On-screen overlays

- **Title card**: `EP 06 · Executive Summary AI (CFO / CEO)`
- **Section badge** (top of result card): `cfo:cockpit · Thai · 2 ¶`
  vs `ceo:cockpit · English · 1 ¶`.
- **Model chip**: e.g. `qwen3.6:35b-a3b-q4_K_M · 1.8s`.
- **Token counter**: `prompt 412 · completion 187 · $0.0002`.
- **CFO/CEO switcher** (top-right corner): animated cross-fade between the
  two workspaces.

### Expected evidence

```bash
# 1. The KPI snapshot that fed the AI
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -c "
  SELECT kpi_key, kpi_value, captured_at
  FROM cockpit_kpis
  WHERE captured_at::date = CURRENT_DATE
  ORDER BY kpi_key;"

# 2. Both AI invocations from this episode
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -c "
  SELECT actor_id, u.fullname, i.section_key, i.task,
         i.model_id, i.prompt_tokens, i.completion_tokens,
         i.latency_ms, i.created_at
  FROM ai_invocations i
  JOIN users u ON u.id = i.actor_id
  WHERE i.section_key IN ('cfo:cockpit', 'ceo:cockpit', 'ledger:commentary')
    AND i.created_at > NOW() - INTERVAL '15 minutes'
  ORDER BY i.created_at;"

# 3. Trial balance check (must be balanced for the narrative to be safe)
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -c "
  SELECT SUM(debit) AS dr, SUM(credit) AS cr,
         CASE WHEN SUM(debit) = SUM(credit) THEN 'BALANCED'
              ELSE 'MISMATCH' END AS status
  FROM journal_lines;"

# 4. Projection scenarios (if stored)
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -c "
  SELECT scenario, period, projected_amount, generated_at
  FROM financial_projections
  WHERE generated_at > NOW() - INTERVAL '1 hour'
  ORDER BY scenario, period;"
```

---

## EP 07 — Chat per Staff Role (results compared)

> "ถามคำถามเดียวกัน — 5 คนได้ 5 คำตอบ" /
> "Same question, five personas, five answers."

| Field          | Value                                                       |
|----------------|-------------------------------------------------------------|
| Personas       | 🪪 John, Andrew, Emily, Charles, Alex (5 of 6)              |
| Trigger        | `AiActionButton` on each persona's current tile             |
| AI section keys| varies per tile (`tile-submit-expense`, `tile-approve-expense`, `tile-cockpit`, `tile-permissions`, `tile-org-chart`) |
| Component      | `app/src/components/ai/AiActionButton.tsx`            |
| API            | `POST /api/ai/invoke` (`app/src/app/api/ai/invoke/route.ts`) |
| Tables touched | `ai_invocations`, `ai_settings.assignments`                 |
| Length target  | ~6 min                                                      |

### How the AI button works

`AiActionButton.tsx:71` POSTs `{ sectionKey, task, text, systemPrompt? }`
to `/api/ai/invoke`. ai-svc resolves the model from
`ai_settings.assignments` keyed by `(section_key, staff_level)` — so the
same `sectionKey` returns different models depending on the actor.

### Demo steps — same question, 5 personas

1. The host types the literal question on screen:
   *"Show me pending approvals over 50,000 THB"*
2. **John** (L5). On `/submit-expense`, the AI button is small and
   greyed out for L5 — system prompt comes back saying scope is "your own
   submissions only". Result: 0 rows (John hasn't submitted anything that
   big).
3. **Andrew** (L4). On `/approve-expense`. AI returns 2 rows: both his
   direct reports' submissions, with note "scope: supervisor_review only".
4. **Emily** (L3). On `/review-queue`. AI returns 7 rows: full dept scope
   (Finance & Account), COA suggestions attached to each.
5. **Charles** (L1). On `/cockpit`. AI returns 23 rows: company-wide, with
   variance flag on the top 3.
6. **Alex** (L2 admin). On `/permissions`. AI returns 23 rows + matrix
   diff annotation: "this approval was made possible by cell_grant X".

### Demo steps — model assignment differs by level

1. Open the **AI Settings** tile (`/settings`, only Alex can see this).
2. The **Assignments** tab
   (`components/ai/SectionsPane.tsx` + `app/src/app/api/ai/assignments/route.ts`)
   shows a table: rows = section keys, columns = staff levels.
3. Highlight:
   - L1 → `qwen3.6:35b-a3b-q4_K_M` (largest, slowest, best reasoning)
   - L2-L3 → `qwen2.5:7b` (balanced)
   - L4-L5 → `qwen2.5:7b` (smaller context window)
4. Click **🔄 Reassign** for `tile-approve-expense` at L5 → switch to
   `qwen2.5:7b` from `qwen3.6:35b`. Save.
5. **Switch back to John** (L5) → run the same question →
   `ai_invocations` row now shows the smaller model + faster latency.

### Demo steps — role-aware system prompts

1. Open `/policy` (Emily's tile). Paste a draft policy: *"Approve all
   expenses > 1M THB by CEO."*
2. Click **🪄 Review Policy** → section `policy:editor`. Result:
   contradiction flagged ("overlaps with CFO review stage for amounts
   > 500K").
3. Open the same tile as Charles (he sees it too). Same draft, same
   button. The AI response is identical because the section key is the
   same — but his persona badge is `L1 CEO`.

### Narration

🇹🇭
> "ปุ่ม AI ฝังอยู่ในแทบทุก tile — กดแล้วยิงไปที่ /api/ai/invoke ระบบจะ
> เลือกโมเดลจากตาราง assignment ตาม section key + staff level ของผู้ใช้
> ถามคำถามเดียวกัน — John L5 เห็นแค่ของตัวเอง, Andrew L4 เห็นของทีม,
> Emily L3 เห็นทั้งแผนก, Charles L1 เห็นทั้งบริษัท นี่คือ RBAC ที่ทำงาน
> บนชั้น prompt ไม่ใช่แค่ชั้น UI"

🇬🇧
> "The AI button is embedded in nearly every tile. Click it and the call
> hits `/api/ai/invoke`; the model is picked from the assignment table by
> `(section_key, staff_level)`. Same question — John (L5) sees only his
> own, Andrew (L4) sees his team, Emily (L3) sees her department,
> Charles (L1) sees the whole company. RBAC enforced at the prompt layer,
> not just the UI."

### On-screen overlays

- **Title card**: `EP 07 · Chat per Staff Role (results compared)`
- **Persona badge** swap at each cut (5 badges, 5 cuts).
- **Result card** model chip changes colour by model size (large = gold,
  medium = cyan, small = slate).
- **Comparison strip** at end: 5 answer cards side-by-side, with row
  counts and "scope:" annotation.
- **Assignment table** (step 2 of model section): colour-graded grid
  (L1 dark → L5 light).

### Expected evidence

```bash
# 1. The five invocations from the same question
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -c "
  SELECT i.actor_id, u.fullname, u.staff_level,
         i.section_key, i.model_id,
         i.prompt_tokens, i.completion_tokens, i.latency_ms
  FROM ai_invocations i
  JOIN users u ON u.id = i.actor_id
  WHERE i.created_at > NOW() - INTERVAL '10 minutes'
  ORDER BY u.staff_level;"

# 2. Assignment table snapshot
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -c "
  SELECT section_key, staff_level, model_id, updated_at
  FROM ai_settings.assignments
  WHERE section_key LIKE 'tile-%'
  ORDER BY section_key, staff_level;"

# 3. The model that actually served John's last call
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -c "
  SELECT section_key, model_id, latency_ms
  FROM ai_invocations
  WHERE actor_id = 1
  ORDER BY created_at DESC LIMIT 1;"

# 4. RBAC-scope proof — Charles sees more rows than John
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -c "
  SELECT 'charles' AS persona, COUNT(*) AS pending_over_50k
  FROM expenses
  WHERE status IN ('pending', 'supervisor_review', 'head_review', 'cfo_review', 'ceo_review')
    AND total_amount > 50000
  UNION ALL
  SELECT 'john', COUNT(*)
  FROM expenses
  WHERE submitter_id = 1
    AND status IN ('pending', 'supervisor_review', 'head_review', 'cfo_review', 'ceo_review')
    AND total_amount > 50000;"
```

---

## Appendix A — Evidence commands (master list)

```bash
# Persona roster (the 6 active personas)
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -c "
  SELECT u.id, u.fullname, u.department, r.name AS role, u.staff_level
  FROM users u
  JOIN roles r ON r.id = u.role_id
  WHERE u.id IN (1, 4, 15, 20, 27, 29)
  ORDER BY u.id;"

# All perm grants touched in the last hour
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -c "
  SELECT role_id, permission_id, granted_at, granted_by
  FROM perm.role_permissions
  WHERE granted_at > NOW() - INTERVAL '1 hour'
  ORDER BY granted_at DESC;"

# All slips uploaded in the last hour
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -c "
  SELECT id, file_path, mime_type, file_size, ocr_confidence,
         uploaded_by, uploaded_at
  FROM slips
  WHERE uploaded_at > NOW() - INTERVAL '1 hour'
  ORDER BY uploaded_at DESC;"

# AI invocations grouped by section + level
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -c "
  SELECT i.section_key, u.staff_level,
         COUNT(*) AS calls,
         AVG(i.latency_ms)::int AS avg_ms,
         SUM(i.prompt_tokens) AS total_prompt_tokens,
         SUM(i.completion_tokens) AS total_completion_tokens
  FROM ai_invocations i
  JOIN users u ON u.id = i.actor_id
  WHERE i.created_at > NOW() - INTERVAL '1 hour'
  GROUP BY i.section_key, u.staff_level
  ORDER BY i.section_key, u.staff_level;"
```

## Appendix B — Section key registry

From `lib/ai/sections.ts`. The full registry that `ai-svc` resolves against
`ai_settings.assignments`:

| Section key           | Task    | Thai label                  | English label                  |
|-----------------------|---------|-----------------------------|--------------------------------|
| `staff:ocr`           | vision  | Receipt Scan (OCR)          | Receipt OCR                    |
| `staff:submit`        | chat    | Expense Form Helper         | Expense submission helper      |
| `acct:coa-search`     | embed   | Map COA Codes with Semantic | COA semantic mapping           |
| `acct:queue`          | chat    | Detect Anomalous Items      | Expense anomaly detection      |
| `hod:approve`         | chat    | Summarize Approver Comments | Approval comment summarizer    |
| `am:review`           | chat    | Recommend Policy            | Policy recommendation          |
| `cfo:cockpit`         | chat    | Executive Summary Narrative | Executive narrative            |
| `ceo:cockpit`         | chat    | Board Summary               | Board summary                  |
| `ledger:commentary`   | chat    | GL Line Commentary          | GL commentary                  |
| `policy:editor`       | chat    | Review Policy               | Policy linting                 |
| `command:intent`      | chat    | Predict Command ⌘K          | Command palette intent         |
| `notification:digest` | chat    | Notification Digest         | Notification digest            |

## Appendix C — Cross-episode index

| Episode | Personas              | Tiles touched                                            | AI sections fired                              |
|---------|-----------------------|----------------------------------------------------------|------------------------------------------------|
| EP 01   | John                  | `submit-expense`                                         | `staff:ocr`                                    |
| EP 02   | John → Andrew → Emily → Charles | `submit-expense`, `approve-expense`, `review-queue`, `search-coa` | `hod:approve`, `am:review`, `acct:queue`, `acct:coa-search` |
| EP 03   | Alex (+ Emily)        | `permissions`, `policy`                                  | `policy:editor`                                |
| EP 04   | Alex                  | `org-chart`, `permissions`, `directory`                  | —                                              |
| EP 05   | Alex (+ John / Emily / Charles) | `permissions`, `directory`                    | —                                              |
| EP 06   | Emily, Charles        | `cockpit`, `ledger`                                      | `cfo:cockpit`, `ceo:cockpit`, `ledger:commentary` |
| EP 07   | John, Andrew, Emily, Charles, Alex | each persona's current tile + `settings`     | varies (see step 1)                            |