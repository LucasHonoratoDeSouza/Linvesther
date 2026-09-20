// Canonical JSON encoding for suite `LZK-JCS-SHA256-v1`.
//
// RFC 8785 (JCS) canonicalization is delegated to `canonicalize`, a
// maintained implementation; this module adds the stricter input rules the
// protocol requires beyond bare RFC 8785, which `JSON.parse` alone cannot
// express because duplicate object keys are already resolved (last write
// wins) and float-vs-integer syntax is indistinguishable once parsed into a
// JS `number`: **duplicate object keys and any float-syntax number are
// rejected outright**, at tokenize time, rather than silently accepted or
// resolved. Mirrors crates/commitments/src/canonical.rs; both must accept
// and reject the same inputs and produce identical canonical bytes — see
// fixtures/commitments/vectors.json.
//
// Schema-level string-encoding rules for specific fields (financial
// amounts and timestamps as sign-free, non-zero-padded integer strings)
// are a domain/schema concern for whichever package defines those
// schemas, not this generic codec layer, and are not enforced here.

import canonicalizeValueRfc8785 from "canonicalize";
import { visit, type ParseErrorCode } from "jsonc-parser";

export class CanonicalError extends Error {}

/**
 * Parses `jsonText`, rejecting duplicate object keys and float-syntax
 * numbers (decimal point or exponent), then returns its RFC 8785 canonical
 * UTF-8 byte encoding.
 */
export function canonicalize(jsonText: string): Uint8Array {
  assertStrict(jsonText);
  let value: unknown;
  try {
    value = JSON.parse(jsonText);
  } catch (error) {
    throw new CanonicalError(`invalid JSON: ${(error as Error).message}`);
  }
  return canonicalizeValue(value);
}

/**
 * Canonicalizes an already-parsed, already-trusted value (e.g. one this
 * module built itself for a commitment). Does not re-run strict
 * validation, since a parsed value can no longer contain duplicate keys or
 * float-syntax literals as distinct from integers.
 */
export function canonicalizeValue(value: unknown): Uint8Array {
  const canonical = canonicalizeValueRfc8785(value);
  if (canonical === undefined) {
    throw new CanonicalError("value has no canonical JSON representation (e.g. undefined/function)");
  }
  return new TextEncoder().encode(canonical);
}

/** Walks the raw token stream, discarding values, purely to validate. */
function assertStrict(jsonText: string): void {
  const scopeStack: Array<Set<string>> = [];
  let error: string | null = null;

  const onError = (code: ParseErrorCode, offset: number) => {
    if (!error) {
      error = `JSON syntax error (code ${code}) at offset ${offset}`;
    }
  };

  visit(
    jsonText,
    {
      onObjectBegin: () => {
        scopeStack.push(new Set());
      },
      onObjectEnd: () => {
        scopeStack.pop();
      },
      onObjectProperty: (property) => {
        const scope = scopeStack[scopeStack.length - 1];
        if (scope) {
          if (scope.has(property)) {
            error = error ?? `duplicate object key: ${property}`;
          }
          scope.add(property);
        }
      },
      onLiteralValue: (value, offset, length) => {
        if (typeof value === "number") {
          const raw = jsonText.slice(offset, offset + length);
          if (/[.eE]/.test(raw)) {
            error =
              error ??
              `float-syntax number rejected (financial/timestamp fields must be integer strings): ${raw}`;
          }
        }
      },
      onError,
    },
    { disallowComments: true, allowTrailingComma: false },
  );

  if (error) {
    throw new CanonicalError(error);
  }
}
