import { createDisposableBag } from '../core/disposable.js';
import { bindText, el, on } from '../dom/index.js';
import type { FieldSpec, FormHandle, FormOptions, FormSpec } from './types.js';

interface FieldBinding {
  readonly spec: FieldSpec;
  readonly input: HTMLInputElement | HTMLSelectElement;
  readonly showError: (text: string) => void;
}

/** Schema to DOM. Confines the per-kind differences in reading values to one place. */
function readValue(binding: FieldBinding): unknown {
  const { spec, input } = binding;
  if (spec.kind === 'checkbox') return (input as HTMLInputElement).checked;
  const raw = input.value;
  if (spec.kind === 'number') return raw === '' ? undefined : Number(raw);
  return raw;
}

/** Builds the input element for a field kind. The caller attaches the error node. */
function buildInput(spec: FieldSpec, initial: unknown): HTMLInputElement | HTMLSelectElement {
  let input: HTMLInputElement | HTMLSelectElement;
  if (spec.kind === 'select') {
    const select = el('select', { name: spec.name, id: `f-${spec.name}` });
    for (const option of spec.options ?? []) {
      const node = el('option', { value: option.value }, option.label);
      if (initial !== undefined && String(initial) === option.value) node.selected = true;
      select.appendChild(node);
    }
    input = select;
  } else if (spec.kind === 'checkbox') {
    const checkbox = el('input', { type: 'checkbox', name: spec.name, id: `f-${spec.name}` });
    checkbox.checked = initial === true;
    input = checkbox;
  } else {
    input = el('input', {
      type: spec.kind === 'number' ? 'number' : 'text',
      name: spec.name,
      id: `f-${spec.name}`,
      ...(initial === undefined ? {} : { value: String(initial) }),
    });
  }

  return input;
}

function fieldRow(spec: FieldSpec, input: HTMLElement, errorNode: HTMLElement): HTMLElement {
  const label = el('label', { attrs: { for: `f-${spec.name}` } }, spec.label);
  const help = spec.help === undefined ? null : el('p', { class: 'field-help' }, spec.help);
  return el('div', { class: 'field' }, label, input, help, errorNode);
}

export function renderForm<T>(spec: FormSpec<T>, options: FormOptions<T>): FormHandle {
  const bag = createDisposableBag();
  const form = el('form', { class: 'form' });
  const bindings: FieldBinding[] = [];
  const errorNodes = new Map<string, HTMLElement>();

  for (const fieldSpec of spec.fields) {
    const input = buildInput(fieldSpec, options.initial?.[fieldSpec.name]);
    const errorNode = el('p', { class: 'field-error' });
    errorNodes.set(fieldSpec.name, errorNode);
    bindings.push({ spec: fieldSpec, input, showError: bindText(errorNode) });
    form.appendChild(fieldRow(fieldSpec, input, errorNode));
  }

  const formError = el('p', { class: 'form-error' });
  const setFormError = bindText(formError);
  const submit = el('button', { type: 'submit' }, spec.submitLabel);
  form.append(formError, submit);

  function clearErrors(): void {
    for (const binding of bindings) binding.showError('');
    setFormError('');
  }

  function collect(): Record<string, unknown> {
    const raw: Record<string, unknown> = {};
    for (const binding of bindings) raw[binding.spec.name] = readValue(binding);
    return raw;
  }

  function applyErrors(errors: Readonly<Record<string, string>>): void {
    clearErrors();
    for (const binding of bindings) {
      const message = errors[binding.spec.name];
      if (message !== undefined) binding.showError(message);
    }
    // An error matching no field is shown as a form-level error
    const unmatched = Object.entries(errors).filter(([key]) => !errorNodes.has(key));
    if (unmatched.length > 0) setFormError(unmatched.map(([, message]) => message).join(' '));
  }

  bag.add(
    on(form, 'submit', (event) => {
      event.preventDefault();
      clearErrors();
      const result = spec.validator.validate(collect());
      if (!result.ok) {
        applyErrors(result.errors);
        return;
      }
      submit.disabled = true;
      void Promise.resolve(options.onSubmit(result.value))
        .catch((error: unknown) => {
          setFormError(error instanceof Error ? error.message : '요청에 실패했습니다');
        })
        .finally(() => {
          submit.disabled = false;
        });
    }),
  );

  return {
    element: form,
    setErrors: applyErrors,
    setValues(values) {
      for (const binding of bindings) {
        const value = values[binding.spec.name];
        if (value === undefined) continue;
        if (binding.spec.kind === 'checkbox') {
          (binding.input as HTMLInputElement).checked = value === true;
          continue;
        }
        binding.input.value = String(value);
      }
      clearErrors();
    },
    reset() {
      form.reset();
      clearErrors();
    },
    dispose: bag.dispose,
  };
}
