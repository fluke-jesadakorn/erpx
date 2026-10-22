import { expect, it } from "vitest";
import { attachmentPolicy } from "@erpx/server-files/public/attachments/policy";
import { sensitiveAttachmentPolicies } from "@erpx/server-files/public/attachments/sensitive-policy";

it("keeps the sensitive workforce registry exact and fails closed for hidden HR purposes", () => {
  expect(
    sensitiveAttachmentPolicies
      .map((policy) => `${policy.targetType}:${policy.purposeKey}:${policy.componentKinds.join(",")}`)
      .sort(),
  ).toEqual(
    [
      "benefit_dependent:dependent_proof:record,action",
      "benefit_election:election_document:record,action",
      "benefit_election:provider_proof:record,action",
      "benefit_plan:benefit_booklet:record",
      "hr_evaluation_assessment:evaluation_evidence:record",
      "hr_person:bank_document:record",
      "hr_person:compensation_authorization:record",
      "hr_person:employment_amendment:record",
      "hr_person:employment_contract:record",
      "hr_person:identity_document:record",
      "hr_person:professional_certificate:record",
      "hr_person:tax_document:record",
      "hr_person:work_permit:record",
      "learning_assignment:certificate:record,action",
      "learning_course:course_material:record",
      "payroll_advance:advance_support:record",
      "payroll_final_pay:final_pay_support:record",
      "payroll_run:correction_support:action",
      "people_clearance_task:clearance_evidence:action",
      "people_lifecycle_case:offer_document:record",
      "people_lifecycle_case:offboarding_document:record",
      "people_lifecycle_case:transfer_document:record",
      "talent_candidate:background_document:record",
      "talent_candidate:cv:record",
      "talent_candidate:portfolio:record",
      "talent_candidate:work_sample:record",
      "talent_offer:offer_document:record",
    ].sort(),
  );

  for (const purpose of ["evaluation", "investigation", "internal_hr_memo"])
    expect(() => attachmentPolicy("hr_person", purpose, "record")).toThrow(
      "This file purpose is not registered",
    );
});
