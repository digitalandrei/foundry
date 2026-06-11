# Phase 9 — Security Hardening

**Status:** Not started · refine this plan right before starting.

## Goal

Close the gaps between the implementation and `../SECURITY.md`; verify every
control actually holds.

## Deliverables

- Secrets-at-rest audit: GitLab tokens, OAuth client secrets, secret env
  vars all encrypted; key management documented in `../DEPLOYMENT.md`
- Agent credential rotation exercised end-to-end + periodic rotation policy
- Rate limiting live on `/auth/*` and `/agent/enroll` (Nginx) and verified
- Session hardening review: cookie flags, fixation, CSRF on state-changing
  routes, OAuth `state`/PKCE verified
- Input-bound review: agent upload size limits, pagination caps, payload
  validation on every boundary
- Dependency audit gate (`cargo deny` or `cargo audit`) wired into
  `scripts/check.sh`
- Log scrubbing review: no tokens/credentials in any log path
- `/security-review` run over the codebase; findings fixed or accepted with
  rationale recorded here

## Acceptance

- Every control in `../SECURITY.md` is demonstrably enforced (test or
  documented manual verification per item)
