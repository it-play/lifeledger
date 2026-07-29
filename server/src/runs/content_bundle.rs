use std::collections::{HashMap, HashSet};

use serde::Serialize;
use sha2::{Digest, Sha256};

use super::types::{
    ContentAuthorityKind, ContentBundleDraft, ContentBundleFailure, ContentBundleFailureCode,
    ContentBundleMember, ContentBundlePublication, ContentBundleRules,
};

pub(super) struct DefaultContentBundleRules;

impl ContentBundleRules for DefaultContentBundleRules {
    fn validate(
        &self,
        draft: &ContentBundleDraft,
    ) -> Result<ContentBundlePublication, Vec<ContentBundleFailure>> {
        validate(draft)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CanonicalBundle<'a> {
    bundle_key: &'a str,
    members: Vec<CanonicalMember<'a>>,
    ranked_eligible: bool,
    schema_version: u16,
    source_note: &'a str,
    version: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CanonicalMember<'a> {
    authority_id: String,
    authority_key: &'a str,
    authority_kind: ContentAuthorityKind,
    authority_sha256: Option<&'a str>,
    authority_version: u32,
    source_note: &'a str,
}

fn validate(
    draft: &ContentBundleDraft,
) -> Result<ContentBundlePublication, Vec<ContentBundleFailure>> {
    let mut failures = Vec::new();
    if !valid_key(&draft.bundle_key)
        || draft.version == 0
        || draft.schema_version == 0
        || draft.source_note.is_empty()
        || draft.members.is_empty()
    {
        failures.push(failure(ContentBundleFailureCode::InvalidBundle, None));
    }

    let mut identities = HashSet::new();
    let mut digests = HashSet::new();
    let mut counts = HashMap::<ContentAuthorityKind, usize>::new();
    for member in &draft.members {
        *counts.entry(member.authority_kind).or_default() += 1;
        let identity = (
            member.authority_kind,
            member.authority_key.as_str(),
            member.authority_version,
        );
        if !identities.insert(identity) {
            failures.push(failure(
                ContentBundleFailureCode::DuplicateAuthorityVersion,
                Some(member),
            ));
        }
        if !valid_key(&member.authority_key)
            || member.authority_version == 0
            || member.source_note.is_empty()
        {
            failures.push(failure(
                ContentBundleFailureCode::InvalidMember,
                Some(member),
            ));
        }
        if !member.referenced {
            failures.push(failure(
                ContentBundleFailureCode::MissingReference,
                Some(member),
            ));
        } else if !member.sealed {
            failures.push(failure(
                ContentBundleFailureCode::UnsealedReference,
                Some(member),
            ));
        }
        match member.authority_sha256.as_deref() {
            Some(digest) if valid_sha256(digest) => {
                if !digests.insert(digest) {
                    failures.push(failure(
                        ContentBundleFailureCode::DuplicateCanonicalSha,
                        Some(member),
                    ));
                }
            }
            Some(_) => failures.push(failure(
                ContentBundleFailureCode::InvalidMember,
                Some(member),
            )),
            None if !legacy_digest_may_be_missing(member.authority_kind, draft) => {
                failures.push(failure(
                    ContentBundleFailureCode::MissingCanonicalSha,
                    Some(member),
                ));
            }
            None => {}
        }
        if draft.ranked_eligible && !member.ranked_eligible {
            failures.push(failure(
                ContentBundleFailureCode::RankedIneligibleAuthority,
                Some(member),
            ));
        }
    }
    validate_cardinality(&counts, &mut failures);
    deduplicate(&mut failures);
    if !failures.is_empty() {
        return Err(failures);
    }

    let mut ordered = draft.members.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| {
        authority_rank(left.authority_kind)
            .cmp(&authority_rank(right.authority_kind))
            .then_with(|| {
                left.authority_key
                    .as_bytes()
                    .cmp(right.authority_key.as_bytes())
            })
            .then_with(|| left.authority_version.cmp(&right.authority_version))
            .then_with(|| left.authority_id.get().cmp(&right.authority_id.get()))
    });
    let canonical = CanonicalBundle {
        bundle_key: &draft.bundle_key,
        members: ordered
            .into_iter()
            .map(|member| CanonicalMember {
                authority_id: member.authority_id.get().to_string(),
                authority_key: &member.authority_key,
                authority_kind: member.authority_kind,
                authority_sha256: member.authority_sha256.as_deref(),
                authority_version: member.authority_version,
                source_note: &member.source_note,
            })
            .collect(),
        ranked_eligible: draft.ranked_eligible,
        schema_version: draft.schema_version,
        source_note: &draft.source_note,
        version: draft.version,
    };
    let canonical_json = serde_json::to_string(&canonical)
        .expect("content bundle canonical projection contains only serializable fields");
    let canonical_sha256 = format!("{:x}", Sha256::digest(canonical_json.as_bytes()));

    Ok(ContentBundlePublication {
        canonical_json,
        canonical_sha256,
    })
}

fn validate_cardinality(
    counts: &HashMap<ContentAuthorityKind, usize>,
    failures: &mut Vec<ContentBundleFailure>,
) {
    for kind in [
        ContentAuthorityKind::CareerCatalog,
        ContentAuthorityKind::RecruitmentRuleset,
        ContentAuthorityKind::EmploymentPolicy,
        ContentAuthorityKind::LifeCatalog,
        ContentAuthorityKind::CreditModel,
        ContentAuthorityKind::RealEstateModel,
        ContentAuthorityKind::PointBudget,
    ] {
        match counts.get(&kind).copied().unwrap_or_default() {
            0 => failures.push(ContentBundleFailure {
                code: ContentBundleFailureCode::MissingAuthorityKind,
                authority_kind: Some(kind),
                authority_id: None,
            }),
            1 => {}
            _ => failures.push(ContentBundleFailure {
                code: ContentBundleFailureCode::InvalidAuthorityCardinality,
                authority_kind: Some(kind),
                authority_id: None,
            }),
        }
    }
    if counts
        .get(&ContentAuthorityKind::CharacterPreset)
        .copied()
        .unwrap_or_default()
        == 0
    {
        failures.push(ContentBundleFailure {
            code: ContentBundleFailureCode::MissingAuthorityKind,
            authority_kind: Some(ContentAuthorityKind::CharacterPreset),
            authority_id: None,
        });
    }
}

fn valid_key(value: &str) -> bool {
    let bytes = value.as_bytes();
    (1..=96).contains(&bytes.len())
        && bytes
            .first()
            .is_some_and(|first| first.is_ascii_lowercase() || first.is_ascii_digit())
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
}

fn legacy_digest_may_be_missing(kind: ContentAuthorityKind, draft: &ContentBundleDraft) -> bool {
    !draft.ranked_eligible
        && matches!(
            kind,
            ContentAuthorityKind::CareerCatalog
                | ContentAuthorityKind::RecruitmentRuleset
                | ContentAuthorityKind::EmploymentPolicy
        )
}

fn authority_rank(kind: ContentAuthorityKind) -> u8 {
    match kind {
        ContentAuthorityKind::CareerCatalog => 10,
        ContentAuthorityKind::RecruitmentRuleset => 20,
        ContentAuthorityKind::EmploymentPolicy => 30,
        ContentAuthorityKind::LifeCatalog => 40,
        ContentAuthorityKind::CreditModel => 50,
        ContentAuthorityKind::RealEstateModel => 60,
        ContentAuthorityKind::CharacterPreset => 70,
        ContentAuthorityKind::PointBudget => 80,
    }
}

fn failure(
    code: ContentBundleFailureCode,
    member: Option<&ContentBundleMember>,
) -> ContentBundleFailure {
    ContentBundleFailure {
        code,
        authority_kind: member.map(|value| value.authority_kind),
        authority_id: member.map(|value| value.authority_id),
    }
}

fn deduplicate(failures: &mut Vec<ContentBundleFailure>) {
    failures.sort_by_key(|failure| {
        (
            failure.code as u8,
            failure.authority_kind.map(authority_rank),
            failure.authority_id.map(|id| id.get()),
        )
    });
    failures.dedup();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finance::ResourceId;
    use crate::runs::create_content_bundle_rules;

    fn given_member(
        authority_kind: ContentAuthorityKind,
        authority_id: u64,
        authority_key: &str,
        authority_sha256: Option<&str>,
    ) -> ContentBundleMember {
        ContentBundleMember {
            authority_kind,
            authority_id: ResourceId::from_u64(authority_id),
            authority_key: authority_key.to_owned(),
            authority_version: 1,
            authority_sha256: authority_sha256.map(str::to_owned),
            source_note: "reviewed typed authority".to_owned(),
            referenced: true,
            sealed: true,
            ranked_eligible: false,
        }
    }

    fn given_valid_draft() -> ContentBundleDraft {
        let digest = |value: char| Some(value.to_string().repeat(64));
        ContentBundleDraft {
            bundle_key: "dev-unranked-content".to_owned(),
            version: 1,
            schema_version: 1,
            ranked_eligible: false,
            source_note: "typed development authorities".to_owned(),
            members: vec![
                given_member(
                    ContentAuthorityKind::PointBudget,
                    8,
                    "budget",
                    digest('8').as_deref(),
                ),
                given_member(
                    ContentAuthorityKind::CharacterPreset,
                    7,
                    "preset",
                    digest('7').as_deref(),
                ),
                given_member(
                    ContentAuthorityKind::RealEstateModel,
                    6,
                    "real-estate",
                    digest('6').as_deref(),
                ),
                given_member(
                    ContentAuthorityKind::CreditModel,
                    5,
                    "credit",
                    digest('5').as_deref(),
                ),
                given_member(
                    ContentAuthorityKind::LifeCatalog,
                    4,
                    "life",
                    digest('4').as_deref(),
                ),
                given_member(
                    ContentAuthorityKind::EmploymentPolicy,
                    3,
                    "employment",
                    None,
                ),
                given_member(
                    ContentAuthorityKind::RecruitmentRuleset,
                    2,
                    "recruitment",
                    None,
                ),
                given_member(ContentAuthorityKind::CareerCatalog, 1, "career", None),
            ],
        }
    }

    mod context_a_complete_unranked_bundle {
        use super::*;

        #[test]
        fn given_shuffled_members_when_validated_then_canonical_order_is_stable() {
            let given = given_valid_draft();

            let when = create_content_bundle_rules()
                .validate(&given)
                .expect("완전한 development bundle은 게시 가능해야 한다");

            assert!(when.canonical_json.find("career").is_some_and(|career| {
                when.canonical_json
                    .find("pointBudget")
                    .is_some_and(|budget| career < budget)
            }));
            assert_eq!(when.canonical_sha256.len(), 64);
        }

        #[test]
        fn given_same_members_in_reverse_when_validated_then_hash_is_identical() {
            let given = given_valid_draft();
            let mut reversed = given.clone();
            reversed.members.reverse();

            let when_first = create_content_bundle_rules()
                .validate(&given)
                .expect("기준 bundle은 게시 가능해야 한다");
            let when_reversed = create_content_bundle_rules()
                .validate(&reversed)
                .expect("순서만 바뀐 bundle은 게시 가능해야 한다");

            assert_eq!(when_first, when_reversed);
        }
    }

    mod context_an_invalid_authority_reference {
        use super::*;

        #[test]
        fn given_missing_reference_when_validated_then_publication_is_rejected() {
            let mut given = given_valid_draft();
            given.members[0].referenced = false;

            let when = create_content_bundle_rules()
                .validate(&given)
                .expect_err("없는 원본 권위는 게시를 막아야 한다");

            assert!(
                when.iter()
                    .any(|failure| { failure.code == ContentBundleFailureCode::MissingReference })
            );
        }

        #[test]
        fn given_unsealed_reference_when_validated_then_publication_is_rejected() {
            let mut given = given_valid_draft();
            given.members[0].sealed = false;

            let when = create_content_bundle_rules()
                .validate(&given)
                .expect_err("봉인되지 않은 원본 권위는 게시를 막아야 한다");

            assert!(
                when.iter()
                    .any(|failure| { failure.code == ContentBundleFailureCode::UnsealedReference })
            );
        }
    }

    mod context_duplicate_content_identity {
        use super::*;

        #[test]
        fn given_duplicate_key_version_when_validated_then_publication_is_rejected() {
            let mut given = given_valid_draft();
            let duplicate = given.members[6].clone();
            given.members.push(duplicate);

            let when = create_content_bundle_rules()
                .validate(&given)
                .expect_err("같은 kind/key/version은 게시를 막아야 한다");

            assert!(when.iter().any(|failure| {
                failure.code == ContentBundleFailureCode::DuplicateAuthorityVersion
            }));
        }

        #[test]
        fn given_duplicate_sha_when_validated_then_publication_is_rejected() {
            let mut given = given_valid_draft();
            given.members[0].authority_sha256 = given.members[1].authority_sha256.clone();

            let when = create_content_bundle_rules()
                .validate(&given)
                .expect_err("같은 canonical SHA는 게시를 막아야 한다");

            assert!(when.iter().any(|failure| {
                failure.code == ContentBundleFailureCode::DuplicateCanonicalSha
            }));
        }
    }

    mod context_a_ranked_bundle {
        use super::*;

        #[test]
        fn given_legacy_digest_or_unranked_member_when_validated_then_publication_is_rejected() {
            let mut given = given_valid_draft();
            given.ranked_eligible = true;

            let when = create_content_bundle_rules()
                .validate(&given)
                .expect_err("ranked bundle은 완전한 SHA와 ranked 원본만 허용해야 한다");

            assert!(
                when.iter().any(|failure| {
                    failure.code == ContentBundleFailureCode::MissingCanonicalSha
                })
            );
            assert!(when.iter().any(|failure| {
                failure.code == ContentBundleFailureCode::RankedIneligibleAuthority
            }));
        }
    }
}
