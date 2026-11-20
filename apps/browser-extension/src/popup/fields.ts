/** Classification controls (DOM) for `Field`s; values are read back as `Raw`. */
import type { Field, Raw } from '../lib/attributes';
import type { AttributeValue } from '../lib/contract';
import { t } from '../lib/i18n';

function el<K extends keyof HTMLElementTagNameMap>(tag: K, props: Partial<HTMLElementTagNameMap[K]> = {}): HTMLElementTagNameMap[K] {
  return Object.assign(document.createElement(tag), props);
}

function emptyOption(): HTMLOptionElement {
  return el('option', { value: '', textContent: t('class_none') });
}

/** Replaces `root`'s children with one control per field, pre-filled from `field.initial`. */
export function renderFields(root: HTMLElement, fields: Field[]): void {
  root.replaceChildren();
  for (const f of fields) {
    const id = `attr-${f.attrId}`;
    const wrap = el('div', { className: 'field' });
    wrap.dataset.attr = f.attrId;
    const label = el('label', { htmlFor: id, textContent: f.name + (f.required ? ' *' : '') });
    wrap.append(label);
    const init: AttributeValue | null = f.initial;
    switch (f.kind) {
      case 'ordered_enum':
      case 'enum': {
        const sel = el('select', { id });
        sel.append(emptyOption(), ...f.options.map((o) => el('option', { value: o.id, textContent: o.label })));
        sel.value = init?.type === 'choice' ? init.value : '';
        wrap.append(sel);
        break;
      }
      case 'multi_enum': {
        const group = el('div', { id, className: 'choices' });
        group.setAttribute('role', 'group');
        group.setAttribute('aria-label', f.name);
        const chosen = init?.type === 'choices' ? init.value : [];
        for (const o of f.options) {
          const box = el('input', { type: 'checkbox', value: o.id, checked: chosen.includes(o.id) });
          const l = el('label', { className: 'check' });
          l.append(box, el('span', { textContent: o.label }));
          group.append(l);
        }
        label.removeAttribute('for');
        wrap.append(group);
        break;
      }
      case 'bool': {
        const sel = el('select', { id });
        sel.append(emptyOption(), el('option', { value: 'true', textContent: t('class_yes') }), el('option', { value: 'false', textContent: t('class_no') }));
        sel.value = init?.type === 'bool' ? String(init.value) : '';
        wrap.append(sel);
        break;
      }
      case 'number':
      case 'text':
      case 'date': {
        const type = f.kind === 'text' ? 'text' : f.kind === 'number' ? 'number' : 'date';
        const input = el('input', { id, type });
        if (f.kind === 'number') input.step = 'any';
        if (init && (init.type === 'number' || init.type === 'text' || init.type === 'date')) input.value = String(init.value);
        wrap.append(input);
        break;
      }
    }
    if (f.required) wrap.querySelector('select, input')?.setAttribute('aria-required', 'true');
    root.append(wrap);
  }
}

/** Reads every control back (attribute id → raw value). */
export function readFields(root: HTMLElement, fields: Field[]): Record<string, Raw> {
  const out: Record<string, Raw> = {};
  for (const f of fields) {
    const wrap = [...root.querySelectorAll<HTMLElement>('[data-attr]')].find((w) => w.dataset.attr === f.attrId);
    if (!wrap) continue;
    if (f.kind === 'multi_enum') {
      out[f.attrId] = [...wrap.querySelectorAll<HTMLInputElement>('input[type=checkbox]')].filter((b) => b.checked).map((b) => b.value);
    } else if (f.kind === 'bool') {
      const v = wrap.querySelector<HTMLSelectElement>('select')?.value ?? '';
      out[f.attrId] = v === '' ? null : v === 'true';
    } else {
      out[f.attrId] = wrap.querySelector<HTMLInputElement | HTMLSelectElement>('select, input')?.value ?? '';
    }
  }
  return out;
}
