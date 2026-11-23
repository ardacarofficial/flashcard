<!--
  Leech repair as a guided action (v1.4): "why do I keep forgetting this word?" in plain
  words. The evidence and the proposals are Rust's (`stats.leech_repairs.plan`): the last wrong
  answers with what the learner typed (`typed_answers`, kept by the core), the answer the card
  expects and what the written check also accepts, then each proposed repair with its reason
  and a before/after preview. "Apply" runs `stats.leech_repairs.apply` (one undoable step,
  Ctrl+Z). Below: what happened after earlier repairs (`stats.leech_repairs.list`), with the
  core's honest "few answers yet" caveat. The AI rewrite proposal is shown but runs from the AI
  screens (its preview must be reviewed first).
-->
<script lang="ts">
  import { api } from '$lib/api';
  import type { LeechRepairPlan, LeechRepairProposal, LeechRepairReport } from '$lib/api/types';
  import { formatRelative } from '$lib/format';
  import { t, td } from '$lib/i18n/index.svelte';
  import { Button, Dialog, Wrench } from '$lib/ui';
  import { LoadState } from '../core/kit';
  import { pct } from './format';

  interface Props {
    cardId: string;
    term: string;
    guarded: (fn: () => Promise<unknown>) => Promise<boolean>;
    onclose: () => void;
    /** After a repair was applied (the caller reloads its lists). */
    onapplied?: () => void;
  }

  let { cardId, term, guarded, onclose, onapplied }: Props = $props();

  let plan = $state<LeechRepairPlan | null>(null);
  let report = $state<LeechRepairReport | null>(null);
  let failed = $state(false);
  let done = $state<string | null>(null);
  let busy = $state(false);

  async function load() {
    const ok = await guarded(async () => {
      plan = await api.call('stats.leech_repairs.plan', { card_id: cardId });
      report = await api.call('stats.leech_repairs.list', { card_id: cardId, limit: 10 });
    });
    failed = !ok && !plan;
  }

  async function apply(p: LeechRepairProposal) {
    busy = true;
    const ok = await guarded(() => api.call('stats.leech_repairs.apply', { card_id: cardId, repair: $state.snapshot(p.repair) }));
    busy = false;
    if (!ok) return;
    done = td(`stats.repair.kind.${p.repair.kind}`);
    onapplied?.();
    await load();
  }

  $effect(() => {
    void cardId;
    void load();
  });
</script>

<Dialog open title={t('stats.repair.title', { term })} description={t('stats.repair.intro')} size="lg" {onclose}>
  {#if failed}
    <LoadState state="error" retry={() => void load()} />
  {:else if !plan}
    <LoadState state="loading" lines={4} />
  {:else}
    <div class="repair" data-testid="leech-repair">
      <p class="say">{t('stats.repair.lapses', { n: plan.lapses })}</p>

      <section aria-labelledby="rp-evidence">
        <h3 id="rp-evidence">{t('stats.repair.evidence')}</h3>
        <p class="line">
          {t('stats.repair.expects')} <strong>{plan.answer || '–'}</strong>
          {#if plan.accepted_answers.length > 0}
            <span class="muted"> · {t('stats.repair.also_accepted', { list: plan.accepted_answers.join(', ') })}</span>
          {/if}
        </p>
        {#if plan.failed_reviews.length === 0}
          <p class="muted">{t('stats.repair.no_failures')}</p>
        {:else}
          <ul class="fails">
            {#each plan.failed_reviews as f (f.at)}
              <li>
                <span class="when">{formatRelative(f.at)}</span>
                {#if f.typed_answer !== null}
                  <span>{t('stats.repair.typed')} <q class="typed" data-testid="leech-typed">{f.typed_answer}</q></span>
                {:else}
                  <span class="muted">{t('stats.repair.not_typed')}</span>
                {/if}
              </li>
            {/each}
          </ul>
        {/if}
        {#if !plan.typed_answers_recorded}
          <p class="muted small">{t('stats.repair.typed_off')}</p>
        {/if}
        {#if plan.confusables.length > 0}
          <p class="line">{t('stats.repair.confusables', { list: plan.confusables.map((c) => c.meaning).join(' · ') })}</p>
        {/if}
      </section>

      <section aria-labelledby="rp-fix">
        <h3 id="rp-fix">{t('stats.repair.proposals')}</h3>
        {#if done}<p class="status" role="status" data-testid="leech-repair-done">{t('stats.repair.applied', { what: done })}</p>{/if}
        {#if plan.proposals.length === 0}
          <p class="muted">{t('stats.repair.none')}</p>
        {:else}
          <ul class="props">
            {#each plan.proposals as p, i (i)}
              <li class="prop" data-testid="leech-proposal">
                <div class="prop-head">
                  <div>
                    <strong>{td(`stats.repair.kind.${p.repair.kind}`)}</strong>
                    <p class="muted">{td(`stats.repair.why.${p.why}`)}</p>
                  </div>
                  {#if p.repair.kind === 'ai_rewrite'}
                    <span class="muted small">{t('stats.repair.ai_hint')}</span>
                  {:else}
                    <Button size="sm" variant={i === 0 ? 'primary' : 'secondary'} icon={Wrench} disabled={busy} onclick={() => void apply(p)}>{t('stats.repair.apply')}</Button>
                  {/if}
                </div>
                {#if p.preview.length > 0}
                  <dl class="preview">
                    {#each p.preview as c (c.field)}
                      <div>
                        <dt>{c.field === 'accepted_answers' ? t('stats.repair.accepted_field') : c.field}</dt>
                        <dd><span class="before">{c.before || '–'}</span> → <span class="after">{c.after || '–'}</span></dd>
                      </div>
                    {/each}
                  </dl>
                {/if}
              </li>
            {/each}
          </ul>
        {/if}
      </section>

      {#if report && report.items.length > 0}
        <section aria-labelledby="rp-past">
          <h3 id="rp-past">{t('stats.repair.past')}</h3>
          <ul class="past">
            {#each report.items as o (o.repair.id)}
              <li>
                <span>{td(`stats.repair.kind.${o.repair.kind}`)} · {formatRelative(o.repair.applied_at)}</span>
                <span class="num">{t('stats.repair.before_after', { before: pct(o.before.rate), after: pct(o.after.rate), n: o.after.reviews })}</span>
                {#if o.small_sample}<span class="muted small">{t('stats.repair.small_sample')}</span>{/if}
              </li>
            {/each}
          </ul>
        </section>
      {/if}
      <p class="muted small">{t('stats.repair.undo')}</p>
    </div>
  {/if}
</Dialog>

<style>
  .repair {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .say {
    margin: 0;
    font-size: var(--text-md);
    font-weight: var(--weight-medium);
  }
  h3 {
    margin: 0 0 var(--space-2);
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
  }
  .line {
    margin: 0 0 var(--space-2);
  }
  .muted {
    color: var(--text-2);
  }
  .small {
    font-size: var(--text-xs);
  }
  .fails,
  .props,
  .past {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  .fails li,
  .past li {
    display: flex;
    gap: var(--space-3);
    flex-wrap: wrap;
    align-items: baseline;
    font-size: var(--text-sm);
  }
  .when {
    color: var(--text-2);
    min-width: 7rem;
  }
  .typed {
    font-family: var(--font-mono);
    color: var(--danger);
  }
  .prop {
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    padding: var(--space-3);
    background: var(--surface-1);
  }
  .prop-head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: var(--space-3);
  }
  .prop-head p {
    margin: 2px 0 0;
    font-size: var(--text-sm);
  }
  .preview {
    margin: var(--space-2) 0 0;
    font-size: var(--text-sm);
  }
  .preview div {
    display: flex;
    gap: var(--space-2);
  }
  .preview dt {
    color: var(--text-2);
    min-width: 6rem;
  }
  .preview dd {
    margin: 0;
  }
  .before {
    color: var(--text-2);
    text-decoration: line-through;
  }
  .after {
    font-weight: var(--weight-medium);
  }
  .status {
    margin: 0 0 var(--space-2);
    color: var(--success);
  }
  .num {
    font-family: var(--font-mono);
  }
</style>
