/** Monochrome marks in the accent colour, so every exchange reads the same. */

function BinanceMark({ width = 20 }: { width?: number }) {
  return (
    <svg width={width} height={width} viewBox="0 0 24 24" fill="currentColor" role="img" aria-label="Binance">
      <path d="M16.624 13.9202l2.7175 2.7154-7.353 7.353-7.353-7.352 2.7175-2.7164 4.6355 4.6595 4.6356-4.6595zm4.6366-4.6366L24 12l-2.7154 2.7164L18.5682 12l2.6924-2.7164zm-9.272.001l2.7163 2.6914-2.7164 2.7174v-.001L9.2721 12l2.7164-2.7154zm-9.2722-.001L5.4088 12l-2.6914 2.6924L0 12l2.7164-2.7164zM11.9885.0115l7.353 7.329-2.7174 2.7154-4.6356-4.6356-4.6355 4.6595-2.7174-2.7154 7.353-7.353z" />
    </svg>
  );
}

function CoinbaseMark({ width = 20 }: { width?: number }) {
  return (
    <svg width={width} height={width} viewBox="0 0 24 24" fill="currentColor" role="img" aria-label="Coinbase">
      <path d="M12 2a10 10 0 1 0 0 20c4.9 0 8.9-3.5 9.8-8.1h-5.5a4.4 4.4 0 1 1 0-3.8h5.5C20.9 5.5 16.9 2 12 2z" />
    </svg>
  );
}

function IbkrMark({ width = 20 }: { width?: number }) {
  return (
    <svg width={width} height={width} viewBox="0 0 24 24" fill="currentColor" role="img" aria-label="Interactive Brokers">
      <path d="M4.5 1h6L6.8 12 1 23V13.5z" />
      <path d="M1 23L9 14l6.5 9z" opacity="0.65" />
      <circle cx="16.5" cy="9.5" r="4.2" />
    </svg>
  );
}

export interface ExchangeEntry {
  name: string;
  status: "connected" | "planned";
  mark: React.ReactNode;
}

export const EXCHANGES: ExchangeEntry[] = [
  { name: "Binance", status: "connected", mark: <BinanceMark /> },
  { name: "Coinbase", status: "connected", mark: <CoinbaseMark /> },
  { name: "Interactive Brokers", status: "connected", mark: <IbkrMark /> },
];
