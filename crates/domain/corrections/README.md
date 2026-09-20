# corrections

Corrections and derived invalidation, per the protocol specification
and the protocol specification's "Autoridade e admissibilidade de
correções" section.

## Scope

- `version::apply_correction` never edits in place: it returns the
  original version (digest unchanged, status now `Superseded`) alongside
  the new replacement version, so the original stays retrievable.
- `report::apply_reports` folds any number of `Report` allegations over a
  `Version` and always returns it unchanged — a report carries no
  authority or evidence, so it cannot invalidate anything, regardless of
  how many accumulate.
- `authority::is_admissible` decides admissibility purely from a
  `Correction`'s own evidence. An `OwnerObjection` can be passed in for
  display purposes, but it never changes the result — the owner cannot
  veto validated evidence.
- `dispute::reconcile_conflicting` resolves two authenticated,
  incompatible versions only when given a `Supersession` demonstrated by
  the source/policy; otherwise both versions are preserved as
  `Resolution::Disputed`. There is no parameter for return magnitude,
  author preference, or arrival order, so none of those can factor into
  the resolution.

## Out of scope

This crate does not implement the full correction pipeline (proof
verification, `expectedCorrectionHead` ordering, or marking descendant
metrics as suspended); it models the four admissibility invariants named
in the acceptance criteria as small, independently testable functions.
