# Shared CI policy interface

Requires Python 3.9+ (stdlib), Git, and checkout with `fetch-depth: 0`.
Run from the repository root. The helper never writes GitHub statuses or checks.

```sh
python3 .github/scripts/ci_policy.py plan --profile build
python3 .github/scripts/ci_policy.py plan --profile build --manual-builds
python3 .github/scripts/ci_policy.py record --profile build --output proof.json
python3 .github/scripts/test_ci_policy.py -v
```

Profiles: `build`, `nix`, `firmware`, `native`, `tests`. `plan` reads
`GITHUB_EVENT_PATH`, `GITHUB_EVENT_NAME`, `GITHUB_REF`, `GITHUB_REPOSITORY`,
`GITHUB_WORKFLOW_REF` and the real `HEAD` tree. It appends these step outputs to
`GITHUB_OUTPUT` (or prints them when absent):

- `run=true|false`
- `reason=<controlled text>`
- `source_run=<original full PR run attempt URL, or empty>`

It appends the decision and, on reuse, original run and proof artifact URLs to
`GITHUB_STEP_SUMMARY`. No remote error or credential text is printed. Reuse needs
`GH_TOKEN` or `GITHUB_TOKEN` with base-repository Actions/contents read access.
Local documentation/test selection needs no token.

## Selection and proof rules

### Lightweight automatic checks; manual heavy builds

Build and Nix opt into `plan --manual-builds` (`plan(..., manual_builds=False)`
remains the default for other callers). Their ordinary branch pushes and PRs
still run the unconditional Python selection/workflow contract suite, then skip
heavy jobs with `manual-build-request-required` before diff or API/reuse lookup.
This is a selection result, not full build evidence: no new proof is published.

After an explicit evening-build request, a maintainer uses **Run workflow**
(`workflow_dispatch`) for Build and Nix on the intended ref. Both select full
coverage even for documentation-only changes. There is no scheduled evening
build. Tags, releases, called workflows and unknown events/refs retain the
conservative full path; release/signing workflows are unchanged.

### Default selection and receipt validation

- Without `--manual-builds`, PRs never reuse remote validation. Pushes may reuse
  only a successful, completed base-repository PR run of the same workflow path
  and profile.
- Manual/called workflows, unknown events/refs, releases, and tags run full.
  Release workflows must remain unconditional full validation at the caller.
- When diff selection is reached, diffs use PR `base.sha...head.sha` or push
  `before..after`, NUL-delimited, with rename detection disabled.
  Shallow history, missing history, invalid SHAs,
  new/deleted branches, malformed input and API failures require full validation.
- Documentation means root `README.md`, `README.rst`, `CHANGELOG.md`,
  `CHANGELOG.rst`, `AGENTS.md`, `LICENSE`, or `docs/**/*.md|rst`.
  Unknown files are not documentation. All profiles may skip documentation-only
  changes. `firmware` may additionally skip solely `tests/**` and
  `.github/scripts/test_*.py` changes. Other profiles run test changes.
- Reuse searches at most 50 PR runs of this workflow created in the last seven
  days, with at most 100 artifacts per run. An incomplete/oversized candidate
  set requires full validation; old proofs simply are not candidates.
- Artifact name must be `ci-proof-PROFILE-RUNID-ATTEMPT`. Run/detail identity,
  repository, workflow, status, conclusion, artifact origin/expiry and receipt
  identity must agree. GitHub Git commit lookups must confirm both the successful
  PR run's immutable `head_sha` and the receipt's `checkout_sha` resolve to the
  real current `HEAD` tree. A receipt cannot claim another PR's arbitrary tree.
  A behind-head synthetic merge conservatively requires new full validation.
  Matching head SHA or successful checks alone is never proof.
- Download bounds: 512 KiB HTTP response, 256 KiB proof ZIP, one basename JSON
  entry no larger than 64 KiB. CRC and duplicate JSON keys are checked. Nothing
  is extracted. Signed archive redirects receive no authorization header.

## Caller responsibilities

Gate full jobs on `needs.plan.outputs.run == 'true'`. Keep an aggregate job that
fails when required jobs fail; a policy decision is not a substitute for job
success. Never invoke `record` or publish a proof after a manual-build policy
skip, documentation skip, reuse, partial validation, failed job, or canceled job.
Only after **all** full
jobs for the profile succeed, on the same checkout, create and upload the single
JSON file using `actions/upload-artifact` and the exact artifact name above.
Do not upload unrelated files in that proof artifact. Reruns use their own attempt
number. Reuse must link the original full artifact, not mint another receipt.

`record` additionally requires `GITHUB_RUN_ID` and `GITHUB_RUN_ATTEMPT`. Receipt:

```json
{"schema":1,"repository":"owner/repo","workflow":".github/workflows/build.yml","profile":"build","run_id":123,"run_attempt":1,"checkout_sha":"<git HEAD>","tree":"<git HEAD^{tree}>","scope":"full"}
```

`record` cannot establish that caller jobs ran: its success gate is a workflow
invariant. The helper's offline suite injects remote transport explicitly;
production GitHub API integration and workflow gating require separate review.
