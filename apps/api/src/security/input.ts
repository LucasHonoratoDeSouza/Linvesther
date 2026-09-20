/** A request field the route needs to be a plain, bounded string. Returns
 * `undefined` for anything else (missing, another type, empty, too long), so
 * the route can answer 400 instead of failing inside the worker. */
export function boundedString(value: unknown, maxLength: number): string | undefined {
  if (typeof value !== "string") return undefined;
  const trimmed = value.trim();
  return trimmed.length > 0 && trimmed.length <= maxLength ? trimmed : undefined;
}

/** A list of short strings, or `undefined` if it is not one. */
export function boundedStrings(value: unknown, maxItems: number, maxLength: number): string[] | undefined {
  if (!Array.isArray(value) || value.length > maxItems) return undefined;
  const items: string[] = [];
  for (const item of value) {
    const text = boundedString(item, maxLength);
    if (text === undefined) return undefined;
    items.push(text);
  }
  return items;
}
