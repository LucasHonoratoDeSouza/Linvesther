import type { Pool } from "pg";
import type { ProfileSettings } from "./types.js";

/** Whether an address is in full privacy mode. Nothing saved means public. */
export interface SettingsStore {
  get(address: string): Promise<ProfileSettings>;
  save(address: string, settings: ProfileSettings): Promise<void>;
}

export class MemorySettingsStore implements SettingsStore {
  private readonly settings = new Map<string, ProfileSettings>();

  async get(address: string): Promise<ProfileSettings> {
    return this.settings.get(address.toLowerCase()) ?? { privacyMode: false };
  }

  async save(address: string, settings: ProfileSettings): Promise<void> {
    this.settings.set(address.toLowerCase(), settings);
  }
}

/** The choice survives restarts — privacy mode must not silently switch itself off. */
export class PostgresSettingsStore implements SettingsStore {
  constructor(private readonly pool: Pool) {}

  async ensureSchema(): Promise<void> {
    await this.pool.query(`
      CREATE TABLE IF NOT EXISTS public_profile_privacy (
        address TEXT PRIMARY KEY,
        privacy_mode BOOLEAN NOT NULL
      )`);
  }

  async get(address: string): Promise<ProfileSettings> {
    const result = await this.pool.query<{ privacy_mode: boolean }>("SELECT privacy_mode FROM public_profile_privacy WHERE address = lower($1)", [address]);
    return { privacyMode: result.rows[0]?.privacy_mode ?? false };
  }

  async save(address: string, settings: ProfileSettings): Promise<void> {
    await this.pool.query(
      `INSERT INTO public_profile_privacy (address, privacy_mode) VALUES (lower($1), $2)
       ON CONFLICT (address) DO UPDATE SET privacy_mode = EXCLUDED.privacy_mode`,
      [address, settings.privacyMode],
    );
  }
}
