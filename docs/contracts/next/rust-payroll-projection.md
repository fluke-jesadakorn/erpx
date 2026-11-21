# Native Payroll accounting projection

## Status and ownership

This is an **inactive, non-authoritative finite compatibility candidate** owned by People/Payroll. It implements the two retained pure exports, `payrollAccountingLines` and `payrollNetCostShares`, for the qualified input families below. It does not replace the TypeScript backend or grant Finance posting, approved-run, database, HTTP, SSR or runtime authority. All business-owner completion flags remain false.

The original behavior is defined by [Payroll accounting](../../../packages/server-people_payroll/src/domain/payroll-accounting.ts), its actual [decimal helper](../../../packages/feature-finance/src/lib/finance-money.ts) and [business error](../../../packages/server-shared/src/domain/error.ts), retained byte-identically from checkpoint `a5ed8737a2f05eff1ee9841fda4a094b3926d732`. The development oracle executes both complete exports with those actual dependencies; it does not replace the algorithm with a model.

Rust implementation lives in People/Payroll domain modules, with lexical request reading in its adapter and an explicitly qualified public facade. Domain logic imports no HTTP, SQL, filesystem or environment APIs. Cross-owner implementation imports are absent. Shared numeric and collation dependencies use existing pinned Cargo versions; this component adds no registry versions.

## Qualified behavior and open parity

The reader retains raw JSON number lexemes, UTF-16 strings and lazy field access. The candidate preserves the tested floating-point operation order, source numeric coercions, unbounded integer allocation, signed remainders, negative zero and stable en-US collation. Calendar-valid four-digit ISO dates and explicit UTC/offset forms use the source time clipping behavior.

`COVERAGE` declares `inactive: true`, `non_authoritative: true` and `full_source_api_parity: false`. The following gates remain open:

- **P1:** general JSON/coercion and native-error behavior outside the qualified profile.
- **P2:** broader V8 date grammar and local-time-zone behavior.
- **P3:** complete UTF-16, case and document grammar behavior.

An unsupported input outside this profile produces an explicit compatibility gap. That gap is a migration limitation, not a new product validation or a Payroll business refusal. It cannot waive any mandatory golden case. This component introduces no statutory rules, positivity limits, account validation, date policy or resource caps.

Frontend previews, approved Payroll runs, independently authorized Finance posting and transaction/finality must use their own reviewed application interfaces before activation. The pure projection is not a posting capability.

## Source oracle and regression evidence

The [portable golden fixture](../../../fixtures/people_payroll/rust-accounting/projection.v1.json) contains 117 constructions observed from both complete original exports. The preserved 108- and 115-case fixtures enforce append-only historical observations. Eight further [review probes](../../../fixtures/people_payroll/rust-accounting/review-probes.v1.json) preserve actual source outcomes: six qualified whole-output comparisons and two explicit P1 gaps outside the finite profile.

The independently reviewed private candidate passed six registered native functions, strict owner all-target Clippy and six architecture functions. Review corrected negative-zero comparison, category initialization, lazy gross conversion and error order, numeric lexical coercion, scalar identity, null error text and JSON whitespace handling. The 117 observations and eight source probes were retained; no expected business outcome was rewritten to make Rust pass. The final source review is bound to manifest `00c06b863008f74399b244919eb6881e1fcb2b4cbea1da9dc5408234cfe0c5a0` and acceptance `326e74abaab7ea7f2ff6724c2c898d994c2de2d11251b5807bd83f555a272e5e`.

Portable promotion changes fixture provenance headers, relocates oracle paths, embeds the unchanged review-probe inputs and adjusts the test fixture include path; native algorithms and observations remain unchanged. All case sections remain byte-identical. Those private results alone do not establish PostgreSQL behavior, Finance authority or runtime acceptance. Canonical integration subsequently passed all six registered Payroll functions (zero failed, ignored or filtered), owner all-target strict Clippy and all six architecture functions. The initial canonical formatter check found only module ordering and the relocated include line; formatting was corrected and independently accepted as preserving declarations, literals and assertions. The formatter then passed. Canonical oracle checks, catalogue (727 suites), documentation, structure and architecture checks also passed; none of these checks activates the runtime.

## Development checks

Use the pinned Node 26.11.1 toolchain and installed locked dependencies from the repository root:

```bash
TZ=Asia/Bangkok LANG=en_US.UTF-8 LC_ALL=en_US.UTF-8 node scripts/testing/fixtures/generate-payroll-accounting-oracle.mjs --check
TZ=Asia/Bangkok LANG=en_US.UTF-8 LC_ALL=en_US.UTF-8 node scripts/testing/fixtures/generate-payroll-accounting-review-probes.mjs --check
bun scripts/testing/test.ts unit --suite=people_payroll.rust-payroll-accounting.unit
```

The oracles guard actual source bytes and the installed TypeScript wrapper/engine. They are development tools; production Rust imports no Node or TypeScript runtime. `--check` compares the complete generated fixture without writing it. The commands explicitly select the qualified locale and time zone, including an LC_ALL override for inherited host settings. The generators record the resolved runtime; they do not qualify other locale or time-zone profiles. A second minimal source path with shared installed dependencies was checked for path portability; this is not a fresh independent dependency-install test.

See the [delivery plan](../../production-plan.md), [owner register](../../status/rust-backend-migration.json) and [Payroll contract](payroll-core.md) for the remaining migration gates.
