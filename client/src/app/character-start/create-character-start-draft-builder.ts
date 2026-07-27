import type {
  CharacterStartingLoan,
  LoanProduct,
  LoanProductCatalog,
  LoanProductKind,
} from '../../api/contracts.js';
import type { CharacterStartDraftBuilder } from './types.js';

interface StartingLoanInput {
  readonly kind: Extract<LoanProductKind, 'studentLoan' | 'unsecuredLoan'>;
  readonly amountKrw: number;
  readonly field: 'studentLoanKrw' | 'creditLoanKrw';
}

/** Resolves preset amounts to the one server-published starting product per loan kind. */
export function createCharacterStartDraftBuilder(): CharacterStartDraftBuilder {
  return {
    build(draft, catalog) {
      const { studentLoanKrw, creditLoanKrw, ...character } = draft;
      const inputs: readonly StartingLoanInput[] = [
        { kind: 'studentLoan', amountKrw: studentLoanKrw, field: 'studentLoanKrw' },
        { kind: 'unsecuredLoan', amountKrw: creditLoanKrw, field: 'creditLoanKrw' },
      ];
      const errors: Record<string, string> = {};
      const startingLoans: CharacterStartingLoan[] = [];
      for (const input of inputs) {
        if (input.amountKrw === 0) continue;
        const product = resolveStartingProduct(catalog, input, errors);
        if (product === undefined) continue;
        if (
          input.amountKrw < product.minimumPrincipalKrw ||
          input.amountKrw > product.maximumPrincipalKrw
        ) {
          errors[input.field] =
            `${product.minimumPrincipalKrw.toLocaleString('ko-KR')}원부터 ` +
            `${product.maximumPrincipalKrw.toLocaleString('ko-KR')}원까지 입력할 수 있습니다.`;
          continue;
        }
        startingLoans.push({
          kind: input.kind,
          productVersionId: product.id,
          principalKrw: input.amountKrw,
        });
      }
      if (Object.keys(errors).length > 0) return { ok: false, errors };
      return { ok: true, value: { character, startingLoans } };
    },
  };
}

function resolveStartingProduct(
  catalog: LoanProductCatalog | undefined,
  input: StartingLoanInput,
  errors: Record<string, string>,
): LoanProduct | undefined {
  const matches =
    catalog?.products.filter(
      (product) => product.kind === input.kind && product.startingEligible,
    ) ?? [];
  if (matches.length !== 1) {
    errors[input.field] = '현재 시작 대출 상품을 확정할 수 없습니다. 상품을 다시 불러와 주세요.';
    return undefined;
  }
  const product = matches[0];
  if (product === undefined || product.rateStatus !== 'available') {
    errors[input.field] = '현재 금리를 확정할 수 없어 이 대출로 시작할 수 없습니다.';
    return undefined;
  }
  return product;
}
