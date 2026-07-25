import type { ZodType } from 'zod';
import type { FormValidation, FormValidator } from '../lib/form/index.js';
import type { ResponseDecoder } from '../lib/http/index.js';

/**
 * zod ↔ 라이브러리 경계 어댑터.
 *
 * lib/ 은 zod 를 모른다. 검증기 교체(예: 서버 OpenAPI 코드젠 결과)를 가능하게 하려면
 * 의존 방향이 앱 → lib 한쪽이어야 하고, 그 접착을 이 파일 하나에 모아 둔다.
 */

export function asDecoder<T>(schema: ZodType<T>): ResponseDecoder<T> {
  return { parse: (input) => schema.parse(input) };
}

export function asFormValidator<T>(schema: ZodType<T>): FormValidator<T> {
  return {
    validate(raw): FormValidation<T> {
      const result = schema.safeParse(raw);
      if (result.success) return { ok: true, value: result.data };
      const errors: Record<string, string> = {};
      for (const issue of result.error.issues) {
        const key = issue.path.map(String).join('.');
        // 같은 필드에 여러 오류가 있으면 첫 번째만 보여준다
        if (errors[key] === undefined) errors[key] = issue.message;
      }
      return { ok: false, errors };
    },
  };
}
