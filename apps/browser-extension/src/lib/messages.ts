/** Popup/options ⇄ service worker messages (one owner for the token, outbox and pairing). */
import type { AddInboxItem, DeckDto, EffectiveAttributeDto } from './contract';
import type { FlushReport, Queued } from './outbox';
import type { Pairing, Settings, Status, SubmitResult } from './service';
import type { Result } from './api';

export type Request =
  | { type: 'status' }
  | { type: 'settings' }
  | { type: 'set_port'; port: number }
  | { type: 'pair_start' }
  | { type: 'pair_poll' }
  | { type: 'pair_forget' }
  | { type: 'decks' }
  | { type: 'classifications'; deckId: string }
  | { type: 'submit'; item: Omit<AddInboxItem, 'client_ref'> }
  | { type: 'flush' }
  | { type: 'outbox' }
  | { type: 'outbox_retry' }
  | { type: 'outbox_discard' };

/** Response type per request. */
export type ResponseOf<R extends Request['type']> = {
  status: Status;
  settings: Settings;
  set_port: Settings;
  pair_start: Result<Pairing>;
  pair_poll: Result<Status['pairing'] | 'denied' | 'expired'>;
  pair_forget: null;
  decks: { decks: DeckDto[]; cached: boolean };
  classifications: { list: EffectiveAttributeDto[]; cached: boolean };
  submit: SubmitResult;
  flush: FlushReport;
  outbox: Queued[];
  outbox_retry: number;
  outbox_discard: number;
}[R];

/** Sends a request to the service worker. */
export function send<R extends Request>(req: R): Promise<ResponseOf<R['type']>> {
  return chrome.runtime.sendMessage(req) as Promise<ResponseOf<R['type']>>;
}
