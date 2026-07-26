import type { Disposable } from '../core/types.js';

export type FieldKind = 'text' | 'textarea' | 'number' | 'select' | 'checkbox';

export interface SelectOption {
  readonly value: string;
  readonly label: string;
}

export interface FieldSpec {
  readonly name: string;
  readonly label: string;
  readonly kind: FieldKind;
  readonly help?: string;
  /** Meaningful only when kind is 'select'. */
  readonly options?: readonly SelectOption[];
}

/**
 * A form validator. A zod schema satisfies this shape, so the form module never imports
 * zod itself. Failures must be convertible to per-field messages.
 */
export interface FormValidator<T> {
  validate(raw: Readonly<Record<string, unknown>>): FormValidation<T>;
}

export type FormValidation<T> =
  | { readonly ok: true; readonly value: T }
  | { readonly ok: false; readonly errors: Readonly<Record<string, string>> };

export interface FormSpec<T> {
  readonly fields: readonly FieldSpec[];
  readonly validator: FormValidator<T>;
  readonly submitLabel: string;
  /** Keeps label/input IDs unique when one screen mounts several forms. */
  readonly idPrefix?: string;
}

export interface FormOptions<T> {
  readonly initial?: Readonly<Record<string, unknown>>;
  readonly onSubmit: (value: T) => void | Promise<void>;
}

/**
 * One rendered form, reused by character creation, resumes and job search alike.
 * A screen attaches `element` rather than assembling DOM itself.
 */
export interface FormHandle extends Disposable {
  readonly element: HTMLFormElement;
  /** Displays a server-side validation failure. */
  setErrors(errors: Readonly<Record<string, string>>): void;
  /** Patches some values, as applying a preset does. Unknown fields are ignored. */
  setValues(values: Readonly<Record<string, unknown>>): void;
  reset(): void;
}
