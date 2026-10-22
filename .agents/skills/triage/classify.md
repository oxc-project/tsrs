# Decide whether the report is actionable

Use the supplied issue body, all comments, and capability table. Later
maintainer or reporter clarifications take precedence. Do not build, install,
run commands, discover skills, reproduce, diagnose, or edit files in this stage.

Return `actionable: false` only when the supplied evidence establishes:

- `not-actionable`: a feature request, question, general performance idea, or
  behavior explicitly listed as unsupported in the capability table;
- `missing-details`: no runnable input/steps or no expected behavior to compare;
- `host-specific` or `unsupported-runtime`: the report fundamentally requires
  a host/runtime unavailable on the Ubuntu runner;
- `maintainer-override`: a `MEMBER`, `COLLABORATOR`, or `OWNER` explicitly says
  not to auto-triage the issue.

Return `actionable: true` when uncertain or when reproduction can resolve a
version question. An old stated version alone never justifies skipping: the
reproduction stage must first try it on the checkout. A concrete performance
regression with runnable public input is actionable.

Call `finish` with exactly `actionable`, `skippedReason`, and a short `reason`.
For actionable reports, `skippedReason` must be null. For skipped reports it
must be one of the reasons above. The action writes the skipped report.
