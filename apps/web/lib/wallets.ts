/** Connecting a wallet reads which addresses it exposes and asks it to sign one
 * message. It never asks for a transaction, an approval or a spending permission,
 * so the only requests this file will make to a wallet are the three below. */
export const ALLOWED_WALLET_METHODS = ["eth_requestAccounts", "eth_chainId", "personal_sign"] as const;
type AllowedMethod = (typeof ALLOWED_WALLET_METHODS)[number];

/** The EIP-1193 provider a browser wallet injects. */
export interface Eip1193Provider {
  request(args: { method: string; params?: unknown[] }): Promise<unknown>;
}

/** What a wallet announces about itself (EIP-6963). */
export interface WalletInfo {
  uuid: string;
  name: string;
  icon: string;
  rdns: string;
}

export interface DiscoveredWallet {
  info: WalletInfo;
  provider: Eip1193Provider;
}

/** Lists the wallets installed in this browser. Each announces itself when asked, so no
 * wallet is guessed at and none is contacted until the person picks it. */
export function discoverWallets(onFound: (wallet: DiscoveredWallet) => void): () => void {
  const seen = new Set<string>();
  const listener = (event: Event) => {
    const detail = (event as CustomEvent<DiscoveredWallet>).detail;
    if (!detail?.info?.uuid || !detail.provider || seen.has(detail.info.uuid)) return;
    seen.add(detail.info.uuid);
    onFound(detail);
  };
  window.addEventListener("eip6963:announceProvider", listener);
  window.dispatchEvent(new Event("eip6963:requestProvider"));
  return () => window.removeEventListener("eip6963:announceProvider", listener);
}

function ask<T>(provider: Eip1193Provider, method: AllowedMethod, params?: unknown[]): Promise<T> {
  if (!(ALLOWED_WALLET_METHODS as readonly string[]).includes(method)) {
    throw new Error(`refusing to send ${method} to a wallet`);
  }
  return provider.request({ method, params }) as Promise<T>;
}

export interface WalletSelection {
  address: string;
  chainId: number;
}

/** Asks the wallet which address the person is using, and on which network. */
export async function selectWalletAccount(provider: Eip1193Provider): Promise<WalletSelection> {
  const accounts = await ask<string[]>(provider, "eth_requestAccounts");
  const address = accounts[0];
  if (!address || !/^0x[0-9a-fA-F]{40}$/.test(address)) throw new Error("The wallet did not share an address.");
  const chain = await ask<string>(provider, "eth_chainId");
  const chainId = Number.parseInt(chain, 16);
  if (!Number.isSafeInteger(chainId) || chainId < 1) throw new Error("The wallet did not say which network it is on.");
  return { address, chainId };
}

/** Has the wallet sign the message: a login-style signature that moves nothing. */
export function signWalletMessage(provider: Eip1193Provider, address: string, message: string): Promise<string> {
  return ask<string>(provider, "personal_sign", [message, address]);
}
