/// <reference types="svelte" />
/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** "1" = use the in-memory mock transport (dev only, never in production builds). */
  readonly VITE_FC_MOCK?: string;
  /** `vite --mode bridge` (dev only): base URL of `fc dev serve`, e.g. `http://127.0.0.1:41234`. */
  readonly VITE_FC_BRIDGE_URL?: string;
  /** `vite --mode bridge` (dev only): the token `fc dev serve` printed. */
  readonly VITE_FC_BRIDGE_TOKEN?: string;
}
interface ImportMeta {
  readonly env: ImportMetaEnv;
}
