/** Adds money exactly.
 *
 * The worker answers with decimal strings (`rust_decimal`), at whatever
 * scale each figure really has. Adding them as JavaScript numbers would
 * round quietly — `0.1 + 0.2` is not `0.3` — so the digits are added as
 * integers and the result is written back at the widest scale any input
 * carried. Nothing is truncated and nothing is rounded.
 *
 * `null` for any input that is not a plain decimal figure: a total that
 * silently skipped a value would read as real while being wrong.
 */
const DECIMAL = /^(-?)(\d+)(?:\.(\d+))?$/;

export function sumDecimals(values: string[]): string | null {
  let scale = 0;
  const parsed: { sign: bigint; whole: string; fraction: string }[] = [];
  for (const value of values) {
    const match = DECIMAL.exec(value.trim());
    if (!match) return null;
    const [, sign, whole, fraction = ""] = match;
    parsed.push({ sign: sign === "-" ? -1n : 1n, whole: whole ?? "0", fraction });
    scale = Math.max(scale, fraction.length);
  }
  let total = 0n;
  for (const { sign, whole, fraction } of parsed) {
    total += sign * BigInt(`${whole}${fraction.padEnd(scale, "0")}`);
  }
  return render(total, scale);
}

function render(total: bigint, scale: number): string {
  const sign = total < 0n ? "-" : "";
  const digits = (total < 0n ? -total : total).toString().padStart(scale + 1, "0");
  if (scale === 0) return `${sign}${digits}`;
  return `${sign}${digits.slice(0, digits.length - scale)}.${digits.slice(digits.length - scale)}`;
}
