export const MIN_PASSWORD_LENGTH = 10;

/** Why a new password is not acceptable, or `null` if it is. The password is
 * the only thing between a stolen backup file and the key inside it, and that
 * key can be attacked offline with no limit on guesses, so a short one is not
 * enough. Applied when choosing a password, never when unlocking one made
 * earlier. */
export function passwordProblem(password: string): string | null {
  if (password.length === 0) return "Enter a password first.";
  if ([...password].length < MIN_PASSWORD_LENGTH) {
    return `Use at least ${MIN_PASSWORD_LENGTH} characters. This password protects your key, and a short one can be guessed offline.`;
  }
  return null;
}
