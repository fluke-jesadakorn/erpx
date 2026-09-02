# Verification and Evidence Catalog

## Deterministic requirement evidence

Every normative leaf requirement has exactly one required evidence package ID:

```text
REQ-<AREA>-<NUMBER><SUFFIX> -> EVD-<AREA>-<NUMBER><SUFFIX>-01
```

The package must contain:

1. the requirement and source/FSD versions;
2. success-path evidence;
3. denial/authorization-path evidence;
4. failure/recovery-path evidence;
5. authoritative state and read-model reconciliation where applicable;
6. audit/correlation identifiers with secrets and unrelated data excluded;
7. execution environment, channel, provider, and policy versions; and
8. the required role sign-offs below.

The deterministic mapping makes every evidence ID unique without maintaining a
second hand-written requirement registry. The validator materializes and checks
the complete current definition set on every run.

## Verification methods and sign-off

| Requirement prefixes | Primary verification | Required sign-off roles |
|---|---|---|
| `FND`, `POL`, `APR`, `WRK` | Scenario demonstration plus contract/decision inspection | Product Council, Architecture, affected Domain Owner |
| `IAM`, `SEC`, `AUD`, `SQA` | Security test, denial test, audit inspection, recovery exercise | Security, Architecture, Identity Domain Owner |
| `PRI`, `REG`, `THA` | Policy analysis, privacy test, deletion/retention exercise, external certification where claimed | Privacy, Legal, Security, external certifier for pack claims |
| `UX`, `NTF`, `ESG` | Multichannel demonstration, accessibility test, adapter/reconciliation test | UX, Product Council, Security; Legal for e-signature claims |
| `PAD`, `ORG`, `COM`, `BRD` | Domain lifecycle test, authorization denial, provider/projection reconciliation | Respective Domain Owner, Architecture, Operations |
| `CUS`, `C360`, `SUP` | Domain lifecycle test, customer-boundary denial, customer-visible audit | Customer/Support Domain Owners, Security, Privacy |
| `FIN`, `SAL`, `POS` | Balanced lifecycle test, reversal/denial, cross-domain reconciliation | Finance/Sales/POS Domain Owners, Architecture |
| `PRC`, `INV`, `AST`, `MNT`, `QUA`, `OPS` | Lifecycle, fixed-precision, source-ownership, recovery, reconciliation test | Respective Domain Owners, Operations, Architecture |
| `PEO`, `LEG`, `TAL`, `LRN`, `BEN` | Restricted-data lifecycle, SoD, effective-date, handoff, retention test | Respective Domain Owners, Privacy, Legal/Security as applicable |
| `MKT`, `CRM`, `ECM` | Consent/authority, lifecycle, provider callback, handoff test | Marketing/CRM/Commerce Domain Owners, Privacy, Finance where applicable |
| `MFG`, `PLN`, `PRJ`, `FSV` | Domain lifecycle, calculation/replay, handoff, reconciliation, close test | Respective Domain Owners, Architecture, Operations |
| `DAT`, `INT`, `IFC`, `MDM`, `REF`, `SRC` | Contract inspection, property test, isolation denial, projection rebuild | Architecture, owning Domain Owner, Security |
| `RPT`, `IMP` | Format/template test, authorization denial, bulk-job recovery, manifest reconciliation | Product Council, Security, affected Domain Owner |
| `AI`, `AGN`, `COL`, `XTN` | Authorization/leakage evaluation, lifecycle test, moderation/recovery, human-authority demonstration | AI Governance Owner, Security, Privacy, UX, affected Domain Owner |
| `DPL`, `BCP`, `MIG`, `OBS`, `REL`, `PRT` | Deployment/recovery/rollback/exit rehearsal and evidence inspection | Operations, Architecture, Security, Customer Authority where applicable |

## Existing versus required evidence

- **Existing supporting evidence** consists only of repository links named in
  each requirement definition: accepted ADRs, code, protobuf contracts,
  migrations, designs, tests, audits, and runbooks in the source snapshot.
- **Required evidence** is the `EVD-*` package that a future acceptance activity
  must produce. An existing source may contribute to a package but does not by
  itself claim that the target requirement is implemented.
- Requirements whose only authority is an FSD explicitly have no asserted
  implementation evidence in this baseline.

## Evidence outcomes

Evidence packages use `Pass`, `Fail`, `Blocked by authority`, or `Not executed`.
Only `Pass` satisfies acceptance. This outcome belongs to certification or
delivery evidence outside this normative library and is not an implementation
status field in `Plan/`.
