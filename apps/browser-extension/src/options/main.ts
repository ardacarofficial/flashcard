/** Options: port, pairing, outbox (waiting/failed items). */
import { errorText } from '../lib/errors';
import { localize, t } from '../lib/i18n';
import { send } from '../lib/messages';

const $ = <T extends HTMLElement>(id: string): T => document.getElementById(id) as T;

async function render(): Promise<void> {
  const [status, items] = await Promise.all([send({ type: 'status' }), send({ type: 'outbox' })]);
  $<HTMLInputElement>('port').value = String(status.port);
  $('conn-state').textContent = t(status.app === 'online' ? 'conn_online' : 'options_conn_offline');
  $('pair-state').textContent = t(
    status.pairing === 'paired' ? 'options_paired' : status.pairing === 'pending' ? 'pair_waiting' : status.last === 'revoked' ? 'pair_revoked' : 'options_unpaired',
  );
  $<HTMLButtonElement>('forget').disabled = !status.paired;
  $('outbox-state').textContent = t('options_outbox_counts', [String(status.outbox.waiting), String(status.outbox.failed)]);
  const failed = items.filter((q) => q.state === 'failed');
  $('failed-list').replaceChildren(
    ...failed.map((q) => {
      const li = document.createElement('li');
      li.textContent = `${q.item.front ?? ''} — ${q.error ? errorText(q.error) : ''}`;
      return li;
    }),
  );
  $<HTMLButtonElement>('flush').disabled = status.outbox.waiting === 0;
  $<HTMLButtonElement>('retry-failed').disabled = failed.length === 0;
  $<HTMLButtonElement>('discard-failed').disabled = failed.length === 0;
}

localize(document);
$('port-form').addEventListener('submit', (e) => {
  e.preventDefault();
  void send({ type: 'set_port', port: Number($<HTMLInputElement>('port').value) }).then(render);
});
$('forget').addEventListener('click', () => void send({ type: 'pair_forget' }).then(render));
$('flush').addEventListener('click', () => void send({ type: 'flush' }).then(render));
$('retry-failed').addEventListener('click', () => void send({ type: 'outbox_retry' }).then(() => send({ type: 'flush' })).then(render));
$('discard-failed').addEventListener('click', () => {
  if (confirm(t('options_discard_confirm'))) void send({ type: 'outbox_discard' }).then(render);
});
void render();
