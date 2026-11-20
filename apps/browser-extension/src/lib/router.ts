/** Dispatches popup/options messages to the service. */
import type { Request } from './messages';
import type { Service } from './service';

export async function handle(service: Service, req: Request): Promise<unknown> {
  switch (req.type) {
    case 'status':
      return service.status();
    case 'settings':
      return service.settings();
    case 'set_port':
      return service.setPort(req.port);
    case 'pair_start':
      return service.startPairing();
    case 'pair_poll':
      return service.pollPairing();
    case 'pair_forget':
      await service.forgetPairing();
      return null;
    case 'decks':
      return service.decks();
    case 'classifications':
      return service.classifications(req.deckId);
    case 'submit':
      return service.submit(req.item);
    case 'flush':
      return service.flush();
    case 'outbox':
      return service.outboxItems();
    case 'outbox_retry':
      return service.outbox.retryFailed();
    case 'outbox_discard':
      return service.outbox.discardFailed();
  }
}
