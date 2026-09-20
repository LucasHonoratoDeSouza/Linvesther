import type { SVGProps } from "react";

const paths = {
  arrow: "M5 12h14m-6-6 6 6-6 6",
  diagonal: "M6 18 18 6M6 6h12v12",
  chevron: "m6 9 6 6 6-6",
  plus: "M12 5v14M5 12h14",
  close: "m6 6 12 12M6 18 18 6",
  search: "m16 16 5 5M18 10a8 8 0 1 1-16 0 8 8 0 0 1 16 0",
  shield: "m12 3 8 3v5c0 5-4 8-8 10-4-2-8-5-8-10V6l8-3Zm-4 9 3 3 5-6",
  lock: "M5 10h14v11H5V10Zm3 0V7a4 4 0 0 1 8 0v3m-4 5v2",
  chart: "M4 4v16h16M8 15l4-5 4 2 5-7",
  wallet: "M20 8H4V5h14v3M4 8v12h17V8H4Zm12 4h5v4h-5v-4Z",
  book: "M12 5v16m0-16C9 3 5 3 2 4v15c3-1 7-1 10 2 3-3 7-3 10-2V4c-3-1-7-1-10 1Z",
  key: "M14 10a5 5 0 1 0-4 4l3 3h3v3h4v-4l-6-6Z",
  check: "m5 12 4 4L19 6",
  copy: "M9 8h12v13H9V8ZM5 16H3V3h12v2",
  code: "m8 6-6 6 6 6m8-12 6 6-6 6M14 3l-4 18",
  menu: "M4 7h16M4 12h16M4 17h16",
  globe:
    "M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0ZM3 12h18M12 3c5 5 5 13 0 18-5-5-5-13 0-18Z",
  play: "m9 5 11 7-11 7V5Z",
  pause: "M8 5v14M16 5v14",
} as const;

export type IconName = keyof typeof paths;

export function Icon({
  name,
  ...props
}: SVGProps<SVGSVGElement> & { name: IconName }) {
  return (
    <svg
      width="20"
      height="20"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      {...props}
    >
      <path d={paths[name]} />
    </svg>
  );
}
