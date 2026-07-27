import type {
  CharacterDraft,
  CharacterStartV2Draft,
  LoanProductCatalog,
} from '../../api/contracts.js';

export type CharacterStartDraftBuildResult =
  | { readonly ok: true; readonly value: CharacterStartV2Draft }
  | { readonly ok: false; readonly errors: Readonly<Record<string, string>> };

export interface CharacterStartDraftBuilder {
  build(
    draft: CharacterDraft,
    catalog: LoanProductCatalog | undefined,
  ): CharacterStartDraftBuildResult;
}
