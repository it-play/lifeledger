import type { Disposable } from '../core/types.js';

export type FieldKind = 'text' | 'number' | 'select' | 'checkbox';

export interface SelectOption {
  readonly value: string;
  readonly label: string;
}

export interface FieldSpec {
  readonly name: string;
  readonly label: string;
  readonly kind: FieldKind;
  readonly help?: string;
  /** kind 가 'select' 일 때만 의미가 있다. */
  readonly options?: readonly SelectOption[];
}

/**
 * 폼 검증기. zod 스키마가 이 형태를 만족하므로 폼 모듈은 zod 를 직접 import 하지 않는다.
 * 실패 시 필드별 메시지로 변환할 수 있어야 한다.
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
}

export interface FormOptions<T> {
  readonly initial?: Readonly<Record<string, unknown>>;
  readonly onSubmit: (value: T) => void | Promise<void>;
}

/**
 * 렌더된 폼 하나. 캐릭터 생성·이력서·공고 검색이 모두 이걸 재사용한다.
 * 화면은 DOM 을 직접 조립하지 않고 element 를 붙이기만 한다.
 */
export interface FormHandle extends Disposable {
  readonly element: HTMLFormElement;
  /** 서버측 검증 실패를 표시할 때 사용한다. */
  setErrors(errors: Readonly<Record<string, string>>): void;
  reset(): void;
}
