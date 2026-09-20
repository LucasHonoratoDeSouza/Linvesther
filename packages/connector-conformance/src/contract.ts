// Connector contract, per the protocol specification:
// "QUANDO novo conector/política é habilitado, DEVE passar
// conformidade, ter versão pública e iniciar cobertura prospectiva sem
// importar passado" and the protocol's completeness requirements. This
// package publishes the contract and a conformance suite any connector
// implementation can be run against — it does not itself add support
// for any new institution.

export interface Page<T> {
  items: T[];
  /** `true` only when this page fully covers the requested window with
   * every filter honored — never defaulted, always the connector's own
   * explicit claim. */
  complete: boolean;
}

export interface LedgerEventFixture {
  id: string;
  economicTimeMs: number;
  symbol: string;
  side: "buy" | "sell";
}

export interface ConnectorContract {
  /** The connector's registration timestamp —, coverage
   * starts prospectively from here; nothing with an earlier
   * `economicTimeMs` may be returned as eligible history. */
  registeredAt: number;
  fetchEvents(sinceMs: number, untilMs: number, filter?: { symbol?: string }): Promise<Page<LedgerEventFixture>>;
}
