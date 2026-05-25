---
mode: agent
model: Claude Sonnet 4
---

<!-- markdownlint-disable-file -->

# Implementation Prompt: Missing Credentials And PII Coverage

## Implementation Instructions

### Step 1: Create Changes Tracking File

You WILL create `20260525-missing-creds-pii-coverage-changes.md` in #file:../changes/ if it does not exist.

### Step 2: Execute Implementation

You WILL follow #file:../../.github/instructions/task-implementation.instructions.md if it exists.
You WILL systematically implement #file:../plans/20260525-missing-creds-pii-coverage-plan.instructions.md task-by-task.
You WILL follow ALL project standards and conventions.
You WILL use #file:../research/20260525-missing-creds-pii-coverage-research.md and #file:../details/20260525-missing-creds-pii-coverage-details.md as the verified implementation source.

**CRITICAL**: If ${input:phaseStop:true} is true, you WILL stop after each Phase for user review.
**CRITICAL**: If ${input:taskStop:false} is true, you WILL stop after each Task for user review.

### Step 3: Cleanup

When ALL Phases are checked off (`[x]`) and completed you WILL do the following:

1. You WILL provide a markdown style link and a summary of all changes from #file:../changes/20260525-missing-creds-pii-coverage-changes.md to the user:

   - You WILL keep the overall summary brief
   - You WILL add spacing around any lists
   - You MUST wrap any reference to a file in a markdown style link

2. You WILL provide markdown style links to .copilot-tracking/plans/20260525-missing-creds-pii-coverage-plan.instructions.md, .copilot-tracking/details/20260525-missing-creds-pii-coverage-details.md, and .copilot-tracking/research/20260525-missing-creds-pii-coverage-research.md documents. You WILL recommend cleaning these files up as well.
3. **MANDATORY**: You WILL attempt to delete .copilot-tracking/prompts/implement-missing-creds-pii-coverage.prompt.md

## Success Criteria

- [ ] Changes tracking file created
- [ ] All plan items implemented with working code
- [ ] All detailed specifications satisfied
- [ ] Project conventions followed
- [ ] Changes file updated continuously
- [ ] `cargo test -p cloakpipe-core` passes
- [ ] `cargo test -p cloakpipe-cli test_scan` passes or model-dependent skips are justified
- [ ] `cloakpipe scan assets/example.md` produces masked output with documented leaks fixed