import type { ReactNode } from "react";

const line = { fill: "none", stroke: "currentColor", strokeWidth: 2.5, strokeLinecap: "round", strokeLinejoin: "round" } as const;
const tint = { fill: "currentColor", fillOpacity: 0.12, stroke: "none" } as const;

const nodes: { title: string; text: string; key?: boolean; art: ReactNode }[] = [
  {
    title: "Your exchange",
    text: "Read-only key. Funds never move.",
    art: (
      <svg viewBox="0 0 160 100" aria-hidden="true">
        <rect x="20" y="10" width="120" height="80" rx="14" {...tint} />
        <rect x="20" y="10" width="120" height="80" rx="14" {...line} />
        <path d="M44 66V52M44 76V44M64 70V58M64 60V40M84 72V56M84 64V46M104 58V44M104 50V30" {...line} strokeWidth={5} />
        <circle cx="124" cy="24" r="10" fill="currentColor" />
        <circle cx="124" cy="24" r="3.5" fill="#f1f4e8" />
      </svg>
    ),
  },
  {
    title: "Collector",
    text: "Reads history, signs what it saw.",
    art: (
      <svg viewBox="0 0 160 100" aria-hidden="true">
        <rect x="32" y="8" width="76" height="84" rx="10" {...tint} />
        <rect x="32" y="8" width="76" height="84" rx="10" {...line} />
        <path d="M46 28h48M46 42h48M46 56h28" {...line} />
        <circle cx="112" cy="68" r="22" fill="#f1f4e8" />
        <circle cx="112" cy="68" r="22" {...line} />
        <path d="M102 68l8 8 14-16" {...line} strokeWidth={4} />
      </svg>
    ),
  },
  {
    title: "Zero-knowledge proof",
    text: "Proves the return. Hides the trades.",
    key: true,
    art: (
      <svg viewBox="0 0 160 100" aria-hidden="true">
        <path d="M14 70C34 68 40 44 58 46S82 30 96 24" {...line} strokeWidth={4} />
        <path d="M96 24c12-4 18 6 30 0s18 8 22 0" {...line} strokeDasharray="1 9" strokeWidth={5} opacity={0.7} />
        <path d="M14 70V88h84V70" {...tint} />
        <rect x="98" y="46" width="48" height="42" rx="10" fill="currentColor" />
        <path d="M110 46v-8a12 12 0 0 1 24 0v8" {...line} strokeWidth={4} />
        <circle cx="122" cy="64" r="5" fill="#141a10" />
        <path d="M122 66v8" stroke="#141a10" strokeWidth={4} strokeLinecap="round" />
      </svg>
    ),
  },
  {
    title: "Onchain registry",
    text: "Public, permanent, checkable by anyone.",
    art: (
      <svg viewBox="0 0 160 100" aria-hidden="true">
        <path d="M52 50h16M92 50h16" {...line} strokeDasharray="1 7" strokeWidth={4} />
        <rect x="10" y="28" width="44" height="44" rx="10" {...tint} />
        <rect x="10" y="28" width="44" height="44" rx="10" {...line} />
        <rect x="58" y="28" width="44" height="44" rx="10" fill="currentColor" />
        <rect x="106" y="28" width="44" height="44" rx="10" {...tint} />
        <rect x="106" y="28" width="44" height="44" rx="10" {...line} />
        <path d="M20 42h24M20 52h16M116 42h24M116 52h16" {...line} strokeWidth={3} />
        <path d="M70 50l8 8 12-16" fill="none" stroke="#f1f4e8" strokeWidth={4} strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    ),
  },
];

export function TechSection() {
  return (
    <section className="tech-section" id="technology">
      <div className="container">
        <span className="eyebrow">The technology</span>
        <h2>
          Proof you can check,
          <br />
          <span>without trusting us.</span>
        </h2>
        <div className="tech-pipeline">
          {nodes.map((node) => (
            <div key={node.title} className={`tech-node${node.key ? " is-key" : ""}`}>
              {node.art}
              <h3>{node.title}</h3>
              <p>{node.text}</p>
            </div>
          ))}
        </div>
        <div className="tech-chips">
          <span>Passkey identity</span>
          <span>Smart accounts</span>
          <span>Base</span>
          <span>RISC Zero</span>
          <span>Open source</span>
        </div>
        <div className="tech-actions">
          <a href="/whitepaper" className="button button-ink">
            Explore the protocol
          </a>
          <a href="/docs/concepts#verification" className="button button-outline">
            How verification works
          </a>
        </div>
      </div>
    </section>
  );
}
