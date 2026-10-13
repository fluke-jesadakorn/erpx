# People evaluation owner contract

People owns Skills, Performance & Compensation at `/holdings/{holdingSlug}/people/evaluation`. Talent remains recruitment. Person, Engagement, Position, Staff Level, Learning, Tasks and Projects keep their existing ownership. Research and product defaults are distinguished in [research evidence](../../research/workforce-compensation-research-2026-10-05.md).

## Criteria and review

`hr_evaluation_definitions` owns versioned Skill, Role Profile, Company policy, annual cycle, Pay Band and Market Benchmark. Every revision has an immutable history, actor and reason. Companies explicitly confirm criteria. Opening a January–December cycle pins the policy version; only closing remains possible. Skill profiles pin skill versions and role-period assessments pin role versions. Ratings identify the exact skill version; a revised behavior rubric does not reuse an old score for target readiness. The editor and heatmap read the pinned behavior/name history. Role requirements and actual work are separate. Assessments cannot overlap for the same annual cycle and Engagement. Recorded position or staff-level changes require separate periods; a draft or reopened assessment can revise its period with a reason, preserving the prior dates/profile in immutable history. Cited evidence must fall within the revised period.

Default performance weights are results 50%, quality 30%, collaboration/adaptability 20%, with 3 meaning meets expectations and 4 beginning outperform. HR may revise them before opening a cycle. Missing weighted dimensions yield no performance score. Skill fit is passing requirement weight / total requirement weight; evidence coverage is assessed requirement count / total requirement count. Unknown skills are shown explicitly. Fit requires at least 80%, coverage at least 80%, and every critical skill demonstrated; an unassessed critical skill makes the conclusion insufficient evidence. Confirmed critical gaps remain visible even when coverage is insufficient. Target readiness requires a distinct, selected, confirmed target profile and a human evidence attestation; it is never inferred from high performance or salary.

Assessment workflow is draft → submitted → calibrating → confirmed. Confirming requires an independent Person from creator and latest score editor. Reopening an open-cycle result requires a reason and preserves all prior versions. An outstanding compensation draft prevents reopening. Every entered rating and actual-work assessment cites immutable evidence belonging to that assessment. Manual KPIs, completed Tasks, Project participation and completed Learning are supporting sources; completion never supplies scores automatically. Sources are checked against employee membership and current Company/team scope. File evidence uses the existing scanned private attachment platform with the exact `hr_evaluation_assessment/evaluation_evidence` restricted purpose.

## Comparisons and privacy

All endpoints derive Company and branch/department limits from the selected work context. A `people_manager` sees only the recursive reporting team. Every read validates live IAM authority. Chart data and table data come from one filtered result; table pagination affects only presentation. All matching role periods are graphed when both axes are known, with missing counts and accessible clickable points. Heatmap uses selected people.

Compensation fields are omitted entirely without `people.compensation.read`, including wages, ranges, normalized base, compa-ratio, policy increase limits and compensation drafts. Restricted evaluation files require both compensation and employee-document permissions, preventing an attachment from exposing wages to a score-only reviewer. API, evidence reads, source selectors, file content and AI use the same scope boundaries.

Pay comparisons select approved compensation effective on the chosen comparison date. HR attests dated FTE, full-time hours and paid days; attendance never substitutes for that basis. Monthly pay = monthly actual base / FTE; daily pay = daily rate × confirmed full-time monthly paid days with consistent daily hours; hourly pay = hourly rate × confirmed full-time monthly hours. All bands use monthly FTE amounts and the same currency. Missing or ambiguous approved pay, missing/overlapping bands and incompatible period/basis produce no compa-ratio. Market freshness is shown independently; expired market data never supplies a current market conclusion. The confirmed internal band remains usable independently of market age.

## Pay and AI workflow

A confirmed assessment may produce one linked Compensation Draft per confirmed version under a confirmed Company increase policy and dated wage basis. The draft snapshots the original approved wage/Engagement versions and copies existing components. Its date must be the next future period boundary after the latest approved/posted pay-group run. Independent approval locks the Engagement and wages, checks assessment/policy/basis/wage versions and absence of issued overlapping payroll, closes the original interval on the prior date and approves the new interval atomically. Existing payroll snapshots remain unchanged. All commands use transactional IAM guards, idempotency receipts and audit/outbox evidence. A draft cannot be independently approved through the generic compensation endpoint. The frozen original/proposed pay evidence includes rates, hours, currency and components; copied components also receive existing money snapshots. A stale unapproved draft can be withdrawn with a reason to allow renewed manual review; approved pay remains unchanged.

AI uses the existing Company provider, gateway metering and PostgreSQL jobs. It is disabled until explicitly enabled in the pinned policy. It accepts a selected list of at most 30 people, records authority and source hashes, rechecks access and source/provider before and after generation, validates employee-specific citations, and stores only advisory drafts. Structured results separate supporting evidence, contradictions, unknowns and development/compensation-review/role-review suggestions. AI never changes scores, promotes, sets pay or approves. Compensation suggestions cannot supply numerical amounts. Failed/stale jobs leave manual assessment available. No file contents are sent to AI.

## API

`/api/v1/holdings/{holdingSlug}/people/evaluation` and `/compare` return filtered rows and allowed configuration. Query keys: year, date, cycleId, departmentId, family, levelId, location, engagementId. Unknown scope parameters are rejected.

- `POST /criteria`: create/revise a typed definition, expectedVersion and reason.
- `POST /market-import`: atomically import 1–100 validated market rows, unconfirmed until Company review. The UI parses quoted CSV, previews amounts/source/dates and maps Company staff levels.
- `POST /assessments`: create a dated role-period assessment.
- `GET /assessments/{id}`: scoped evidence, prior results, source choices and authorized wage history.
- `POST /assessments/{id}/save`, `/transition`, `/evidence`, `/compensation`: versioned owner commands.
- `GET /evidence/{id}`: inspect evidence after current scope authorization.
- `POST /basis/{engagementId}`: dated HR attestation.
- `POST /compensation/{id}/approve`: independent atomic interval replacement.
- `POST /compensation/{id}/cancel`: withdraw an unapproved proposal with expectedVersion and reason.
- `POST /ai`, `GET /ai/{id}`: queue and inspect authorized advisory drafts.

Reads use private/no-store responses. Writes validate origin and require Idempotency-Key. Long editors and evidence views use fixed-header right drawers, full width on mobile.
