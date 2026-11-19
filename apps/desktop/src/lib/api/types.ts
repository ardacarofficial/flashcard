/**
 * The single import point for backend DTOs: everything here is generated from `fc-api`
 * (`cargo run -p fc-api --bin gen-ts`). Never hand-write a DTO — change the Rust type and
 * regenerate; `pnpm check` then shows every place the UI must follow.
 */
export type * from './generated';
export type { MethodName, RequestOf, ResponseOf } from './generated/methods';
