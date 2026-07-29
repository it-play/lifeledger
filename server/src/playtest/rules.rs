use super::{
    ConsentAction, ConsentCommand, ConsentStoredStatus, ConsentTransition, FeedbackDraft,
    MAXIMUM_FEEDBACK_CHARACTERS, NormalizedFeedbackDraft, PlaytestFailureCode, PlaytestRules,
    StoredConsent,
};

struct DefaultPlaytestRules;

pub fn create_playtest_rules() -> std::sync::Arc<dyn PlaytestRules> {
    std::sync::Arc::new(DefaultPlaytestRules)
}

impl PlaytestRules for DefaultPlaytestRules {
    fn plan_consent_transition(
        &self,
        active_policy_version_id: u64,
        current: Option<&StoredConsent>,
        command: &ConsentCommand,
    ) -> Result<ConsentTransition, PlaytestFailureCode> {
        if command.policy_version_id != active_policy_version_id {
            return Err(PlaytestFailureCode::PolicyUnavailable);
        }

        let current_revision = current.map_or(0, |consent| consent.revision);
        if command.expected_revision != current_revision {
            return Err(PlaytestFailureCode::RevisionConflict);
        }

        match (command.action, current) {
            (ConsentAction::Withdraw, None) => Err(PlaytestFailureCode::ConsentRequired),
            (ConsentAction::Grant, Some(consent))
                if consent.status == ConsentStoredStatus::Granted
                    && consent.policy_version_id == active_policy_version_id =>
            {
                Ok(ConsentTransition {
                    changed: false,
                    policy_version_id: consent.policy_version_id,
                    status: consent.status,
                    revision: consent.revision,
                })
            }
            (ConsentAction::Withdraw, Some(consent))
                if consent.status == ConsentStoredStatus::Withdrawn =>
            {
                Ok(ConsentTransition {
                    changed: false,
                    policy_version_id: consent.policy_version_id,
                    status: consent.status,
                    revision: consent.revision,
                })
            }
            (action, current) => {
                let revision = current_revision
                    .checked_add(1)
                    .filter(|revision| *revision <= 9_007_199_254_740_991)
                    .ok_or(PlaytestFailureCode::RevisionConflict)?;
                let policy_version_id = match action {
                    ConsentAction::Grant => active_policy_version_id,
                    ConsentAction::Withdraw => current
                        .map(|consent| consent.policy_version_id)
                        .ok_or(PlaytestFailureCode::ConsentRequired)?,
                };

                Ok(ConsentTransition {
                    changed: true,
                    policy_version_id,
                    status: action.as_status(),
                    revision,
                })
            }
        }
    }

    fn normalize_feedback(
        &self,
        draft: FeedbackDraft,
    ) -> Result<NormalizedFeedbackDraft, PlaytestFailureCode> {
        if !draft.privacy_confirmed {
            return Err(PlaytestFailureCode::PrivacyConfirmationRequired);
        }
        if draft.expected_consent_revision == 0 {
            return Err(PlaytestFailureCode::ConsentRequired);
        }

        let message = draft.message.trim();
        let character_count = message.chars().count();
        let has_unsupported_control = message
            .chars()
            .any(|character| character.is_control() && character != '\n' && character != '\t');
        if character_count == 0
            || character_count > MAXIMUM_FEEDBACK_CHARACTERS
            || has_unsupported_control
        {
            return Err(PlaytestFailureCode::InvalidCommand);
        }

        Ok(NormalizedFeedbackDraft {
            expected_consent_revision: draft.expected_consent_revision,
            category: draft.category,
            severity: draft.severity,
            message: message.to_owned(),
            run_revision: draft.run_revision,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playtest::{FeedbackCategory, FeedbackSeverity};

    fn given_rules() -> std::sync::Arc<dyn PlaytestRules> {
        create_playtest_rules()
    }

    fn given_granted_consent(policy_version_id: u64, revision: u64) -> StoredConsent {
        StoredConsent {
            policy_version_id,
            status: ConsentStoredStatus::Granted,
            revision,
            granted_at: "2026-07-30T00:00:00.000000Z".to_owned(),
            withdrawn_at: None,
        }
    }

    mod context_동의_revision이_오래된_경우 {
        use super::*;

        #[test]
        fn given_현재보다_낮은_revision_when_동의를_바꾸면_then_충돌로_거절한다() {
            let rules = given_rules();
            let current = given_granted_consent(3, 2);
            let command = ConsentCommand {
                policy_version_id: 3,
                expected_revision: 1,
                action: ConsentAction::Withdraw,
            };

            let result = rules.plan_consent_transition(3, Some(&current), &command);

            assert_eq!(result, Err(PlaytestFailureCode::RevisionConflict));
        }
    }

    mod context_고지_policy가_바뀐_경우 {
        use super::*;

        #[test]
        fn given_이전_policy_동의_when_다시_동의하면_then_새_policy와_revision을_쓴다() {
            let rules = given_rules();
            let current = given_granted_consent(2, 4);
            let command = ConsentCommand {
                policy_version_id: 3,
                expected_revision: 4,
                action: ConsentAction::Grant,
            };

            let result = rules.plan_consent_transition(3, Some(&current), &command);

            assert_eq!(
                result,
                Ok(ConsentTransition {
                    changed: true,
                    policy_version_id: 3,
                    status: ConsentStoredStatus::Granted,
                    revision: 5,
                })
            );
        }

        #[test]
        fn given_이전_policy_동의_when_철회하면_then_원래_동의한_policy를_보존한다() {
            let rules = given_rules();
            let current = given_granted_consent(2, 4);
            let command = ConsentCommand {
                policy_version_id: 3,
                expected_revision: 4,
                action: ConsentAction::Withdraw,
            };

            let result = rules.plan_consent_transition(3, Some(&current), &command);

            assert_eq!(
                result,
                Ok(ConsentTransition {
                    changed: true,
                    policy_version_id: 2,
                    status: ConsentStoredStatus::Withdrawn,
                    revision: 5,
                })
            );
        }
    }

    mod context_피드백을_정규화하는_경우 {
        use super::*;

        #[test]
        fn given_유효한_본문_when_정규화하면_then_바깥_공백을_제거한다() {
            let rules = given_rules();
            let draft = FeedbackDraft {
                expected_consent_revision: 2,
                category: FeedbackCategory::Bug,
                severity: FeedbackSeverity::Major,
                message: "  계산 결과가 달라요.  ".to_owned(),
                privacy_confirmed: true,
                run_revision: Some(3),
            };

            let result = rules.normalize_feedback(draft);

            assert_eq!(
                result,
                Ok(NormalizedFeedbackDraft {
                    expected_consent_revision: 2,
                    category: FeedbackCategory::Bug,
                    severity: FeedbackSeverity::Major,
                    message: "계산 결과가 달라요.".to_owned(),
                    run_revision: Some(3),
                })
            );
        }

        #[test]
        fn given_개인정보_확인_없음_when_정규화하면_then_제출을_거절한다() {
            let rules = given_rules();
            let draft = FeedbackDraft {
                expected_consent_revision: 2,
                category: FeedbackCategory::Other,
                severity: FeedbackSeverity::Suggestion,
                message: "제안".to_owned(),
                privacy_confirmed: false,
                run_revision: None,
            };

            let result = rules.normalize_feedback(draft);

            assert_eq!(
                result,
                Err(PlaytestFailureCode::PrivacyConfirmationRequired)
            );
        }
    }
}
