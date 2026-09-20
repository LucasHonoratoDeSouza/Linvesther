# Security policy

## Reporting a vulnerability

Please report security issues privately through GitHub's "Report a
vulnerability" form on this repository's Security tab, not in a public issue.
Include what you found, how to reproduce it and the impact you see. Never
include real credentials or account data.

## Scope

In scope: the web application, the API, the smart contracts, the collectors and
the proof and verification code in this repository.

The contracts are deployed on a test network only. Nothing here has had an
independent security review yet.

## What we protect

- Exchange and broker credentials are read-only and are never published.
- No balance, position or trade is exposed by the public API.
- A claim is valid only if the owner authorized its exact content.
