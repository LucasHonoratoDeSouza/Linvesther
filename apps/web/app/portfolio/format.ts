// Number/date formatting for the portfolio — every amount the API sends
// is an exact decimal string (never a float), so it's only converted for
// display, here.

export const money = (value: string, digits = 2) =>
  Number(value).toLocaleString("en-US", { minimumFractionDigits: digits, maximumFractionDigits: digits });

export const signedMoney = (value: string | number, digits = 2) => {
  const n = Number(value);
  return `${n > 0 ? "+" : ""}${n.toLocaleString("en-US", { minimumFractionDigits: digits, maximumFractionDigits: digits })}`;
};

export const percent = (fraction: string, digits = 2) => `${(Number(fraction) * 100).toFixed(digits)}%`;

export const quantity = (value: string) => Number(value).toLocaleString("en-US", { maximumFractionDigits: 6 });

export const dateLabel = (ms: number) =>
  new Date(ms).toLocaleString("en-US", { year: "numeric", month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });

export const priceDigits = (price: string) => (Number(price) < 10 ? 4 : 2);
