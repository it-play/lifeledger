import type { ZodType } from 'zod';
import type { FormValidation, FormValidator } from '../lib/form/index.js';
import type { ResponseDecoder } from '../lib/http/index.js';

/**
 * The zod-to-library boundary adapter.
 *
 * `lib/` knows nothing about zod. Keeping the dependency one-way (app -> lib) is what
 * allows the validator to be swapped later for generated OpenAPI code, and all of that
 * glue lives in this single file.
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
        // Show only the first error per field
        if (errors[key] === undefined) errors[key] = issue.message;
      }
      return { ok: false, errors };
    },
  };
}
