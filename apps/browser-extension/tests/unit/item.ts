import type { AddInboxItem } from '../../src/lib/contract';

/** An `AddInboxItem` with the generated type's always-present collections filled in. */
export function inboxItem<T extends Partial<AddInboxItem>>(p: T): AddInboxItem & T {
  return { fields: {}, tags: [], attributes: {}, ...p };
}
