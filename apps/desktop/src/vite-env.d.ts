/// <reference types="svelte" />
/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** "1" = use the in-memory mock transport (dev only, never in production builds). */
  readonly VITE_FC_MOCK?: string;
}
interface ImportMeta {
  readonly env: ImportMetaEnv;
}
