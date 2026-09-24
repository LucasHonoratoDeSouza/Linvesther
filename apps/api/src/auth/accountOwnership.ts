// Who may read or change one connected account. Shared by every
// owner-gated route, whichever credential got the caller in: the browser
// session behind the connection routes and the read-only token behind
// `/mcp/*` both end up here, so there is one answer to "is this account
// theirs" and no second copy to drift.

/** An address always owns the accountId equal to its own address, and
 * any `<address>_<name>` it adds for further exchange accounts — this
 * lets a signed-in person manage several connections without a prior,
 * separate registration in `accountOwners` (that map still takes
 * precedence when it has an explicit entry). Never grants access to any
 * *other* person's accountId: the prefix must be the caller's own
 * address, and the name part is restricted to a short slug. */
const EXTRA_ACCOUNT_NAME = /^[A-Za-z0-9-]{1,32}$/;

export function isOwner(accountOwners: Map<string, `0x${string}`>, accountId: string, callerAddress: `0x${string}`): boolean {
  const registered = accountOwners.get(accountId);
  if (registered) {
    return registered.toLowerCase() === callerAddress.toLowerCase();
  }
  const caller = callerAddress.toLowerCase();
  const id = accountId.toLowerCase();
  if (id === caller) {
    return true;
  }
  return id.startsWith(`${caller}_`) && EXTRA_ACCOUNT_NAME.test(accountId.slice(callerAddress.length + 1));
}
