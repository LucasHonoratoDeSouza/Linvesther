# connector-conformance

Connector conformance kit, per the protocol specification's
the protocol ("novo conector/política... DEVE passar conformidade... iniciar
cobertura prospectiva sem importar passado") and the protocol's completeness
requirements.

**This package does not add support for any new institution.** It
publishes the `ConnectorContract` interface and a reusable
`runConformanceSuite` any future connector implementation (Binance or
otherwise) can be run against — enabling this kit is not itself a claim
that any new exchange is available.

## Scope

- `contract.ts` — `ConnectorContract`: a `registeredAt` timestamp and
  `fetchEvents(sinceMs, untilMs, filter?)` returning a `Page` that
  always explicitly states whether it's `complete`.
- `suite.ts` — `runConformanceSuite` runs four checks against any
  contract implementation:
  - the full requested window is reported complete;
  - no returned event has `economicTimeMs` before `registeredAt`
    ("backdate" — the protocol's "sem importar passado");
  - filtering by `symbol` actually narrows the results (a connector
    that silently ignores the filter fails);
  - splitting the window into two paginated calls accounts for the same
    events as one full-window call (a connector that drops a page when
    paginated fails, even if each page individually claims
    `complete: true`).

## Tests

`test/syntheticConnector.ts` provides four independent, synthetic
connector implementations, written without reference to `suite.ts`'s
internals: one well-formed, and three each violating exactly one rule
(backdating, filter-ignoring, page-dropping). `test/suite.test.ts`
confirms the well-formed one passes every check and each broken one is
rejected on the specific check it violates.

## Out of scope

No real connector (Binance or otherwise) is implemented against this
contract in this package — `services/collector/binance/*` predates this
kit and is not retrofitted to implement `ConnectorContract` here. This
is the publishable conformance surface a *future* connector would need
to satisfy.
