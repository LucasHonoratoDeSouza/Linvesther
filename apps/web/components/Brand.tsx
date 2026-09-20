import Link from "next/link";

/** The final, fully-drawn frame of the real animated mark (see
 * components/AnimatedMark.tsx for the live version) — same paths,
 * traced by hand from the source canvas animation at its complete
 * state, not a separate design. */
export function Brand() {
  return (
    <Link className="wordmark" href="/" aria-label="Linvesther home">
      <svg width="28" height="28" viewBox="0 0 180 180" fill="none" aria-hidden="true">
        <g stroke="#d8ff91" strokeWidth="18" strokeLinejoin="round">
          <path d="M96,92 C80,92 59,91 36,90" strokeLinecap="round" />
          <path d="M96,92 C111,84 130,68 146,62" strokeLinecap="round" />
          <path d="M96,92 C82,82 70,62 57,51 C50,45 43,43 36,44" strokeLinecap="butt" />
          <path d="M96,92 C82,102 70,121 57,131 C50,137 43,139 35,136" strokeLinecap="butt" />
        </g>
        <circle cx="36" cy="44" r="9" fill="#d8ff91" />
        <circle cx="35" cy="136" r="9" fill="#d8ff91" />
        <circle cx="96" cy="92" r="9" fill="#d8ff91" />
      </svg>
      <span>linvesther</span>
    </Link>
  );
}
