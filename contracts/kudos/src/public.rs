use crate::registry::{ext_sbtreg, IS_HUMAN_GAS};
use crate::settings::Settings;
use crate::types::{Commentary, KudosId, KudosKind, WrappedCid};
use crate::{consts::*, CommentId, EncodedCommentary};
use crate::{utils::*, GIVE_KUDOS_COST};
use crate::{Contract, ContractExt};
use near_sdk::serde_json::Value;
use near_sdk::{env, near_bindgen, require, AccountId, Promise};

#[near_bindgen]
impl Contract {
    /// Allows caller to exchange kudos associated with [`KudosId`] for ProofOfKudos SBT.
    /// Caller should have a valid i-am-human SBT. Every unique [`KudosId`] could be exchanged only once and
    /// only if it has sufficient amount of upvotes. Calls `sbt_mint` of i-am-human-registry contract.
    #[payable]
    #[handle_result]
    pub fn exchange_kudos_for_sbt(&mut self, kudos_id: KudosId) -> Result<Promise, &'static str> {
        self.assert_contract_running();

        let minimum_gas_requirement = EXCHANGE_KUDOS_FOR_SBT_RESERVED_GAS
            + IS_HUMAN_GAS
            + ACQUIRE_NUMBER_OF_UPVOTES_RESERVED_GAS
            + SOCIAL_DB_REQUEST_MIN_RESERVED_GAS
            + KUDOS_UPVOTES_ACQUIRED_CALLBACK_GAS
            + PROOF_OF_KUDOS_SBT_MINT_GAS
            + PROOF_OF_KUDOS_SBT_MINT_CALLBACK_GAS
            + FAILURE_CALLBACK_GAS;
        require!(
            env::prepaid_gas() >= minimum_gas_requirement,
            display_gas_requirement_in_tgas(minimum_gas_requirement)
        );

        let attached_deposit = env::attached_deposit();
        require!(
            attached_deposit == EXCHANGE_KUDOS_COST,
            &display_deposit_requirement_in_near(EXCHANGE_KUDOS_COST)
        );

        if self.exchanged_kudos.contains(&kudos_id) {
            return Err("Kudos is already exchanged");
        }

        let predecessor_account_id = env::predecessor_account_id();
        let external_db_id = self.external_db_id()?.clone();

        let gas_remaining = env::prepaid_gas()
            - (env::used_gas() + IS_HUMAN_GAS + EXCHANGE_KUDOS_FOR_SBT_RESERVED_GAS);

        Ok(ext_sbtreg::ext(self.iah_registry.clone())
            .with_static_gas(IS_HUMAN_GAS)
            .is_human(env::signer_account_id())
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(gas_remaining)
                    .acquire_number_of_upvotes(
                        predecessor_account_id,
                        attached_deposit.into(),
                        external_db_id,
                        kudos_id,
                    ),
            ))
    }

    /// Allows caller to leave a commentary message [`String`] to a kudos associated with [`KudosId`]
    /// for a user by [`AccountId`]. Caller should have a valid i-am-human SBT and can't leave
    /// commentary message for his own kudos.
    #[payable]
    #[handle_result]
    pub fn leave_comment(
        &mut self,
        receiver_id: AccountId,
        kudos_id: KudosId,
        parent_comment_id: Option<CommentId>,
        message: String,
    ) -> Result<Promise, String> {
        self.assert_contract_running();

        let predecessor_account_id = env::predecessor_account_id();
        let sender_id = env::signer_account_id();

        let minimum_gas_requirement = LEAVE_COMMENT_RESERVED_GAS
            + IS_HUMAN_GAS
            + ACQUIRE_KUDOS_INFO_RESERVED_GAS
            + SOCIAL_DB_REQUEST_MIN_RESERVED_GAS
            + KUDOS_INFO_ACQUIRED_CALLBACK_GAS
            + SOCIAL_DB_REQUEST_MIN_RESERVED_GAS
            + KUDOS_COMMENT_SAVED_CALLBACK_GAS
            + FAILURE_CALLBACK_GAS;
        require!(
            env::prepaid_gas() >= minimum_gas_requirement,
            display_gas_requirement_in_tgas(minimum_gas_requirement)
        );

        let attached_deposit = env::attached_deposit();
        require!(
            attached_deposit == LEAVE_COMMENT_COST,
            &display_deposit_requirement_in_near(LEAVE_COMMENT_COST)
        );

        if message.len() > Settings::from(&self.settings).commentary_message_max_length  as usize {
            return Err("Message max length exceeded".to_string());
        }
        let external_db_id = self.external_db_id()?.clone();
        let comment = EncodedCommentary::try_from(&Commentary {
            sender_id: &sender_id,
            message: &Value::String(message),
            timestamp: env::block_timestamp_ms().into(),
            parent_comment_id: parent_comment_id.as_ref(),
        })?;

        let gas_remaining =
            env::prepaid_gas() - (env::used_gas() + IS_HUMAN_GAS + LEAVE_COMMENT_RESERVED_GAS);

        Ok(ext_sbtreg::ext(self.iah_registry.clone())
            .with_static_gas(IS_HUMAN_GAS)
            .is_human(env::signer_account_id())
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(gas_remaining)
                    .acquire_kudos_info(
                        predecessor_account_id,
                        attached_deposit.into(),
                        external_db_id,
                        receiver_id,
                        kudos_id,
                        parent_comment_id,
                        comment,
                    ),
            ))
    }

    /// Allows caller to upvote kudos associated with [`KudosId`] for a user by [`AccountId`].
    /// Caller should have a valid i-am-human SBT and can't upvote his own kudos.
    #[payable]
    #[handle_result]
    pub fn upvote_kudos(
        &mut self,
        receiver_id: AccountId,
        kudos_id: KudosId,
    ) -> Result<Promise, &'static str> {
        self.assert_contract_running();

        let predecessor_account_id = env::predecessor_account_id();
        let sender_id = env::signer_account_id();
        require!(
            receiver_id != sender_id,
            "User is not eligible to upvote this kudos"
        );

        let minimum_gas_requirement = UPVOTE_KUDOS_RESERVED_GAS
            + IS_HUMAN_GAS
            + ACQUIRE_KUDOS_SENDER_RESERVED_GAS
            + SOCIAL_DB_REQUEST_MIN_RESERVED_GAS
            + KUDOS_SENDER_ACQUIRED_CALLBACK_GAS
            + SOCIAL_DB_REQUEST_MIN_RESERVED_GAS
            + KUDOS_UPVOTE_SAVED_CALLBACK_GAS
            + FAILURE_CALLBACK_GAS;
        require!(
            env::prepaid_gas() >= minimum_gas_requirement,
            display_gas_requirement_in_tgas(minimum_gas_requirement)
        );

        let attached_deposit = env::attached_deposit();
        require!(
            attached_deposit == UPVOTE_KUDOS_COST,
            &display_deposit_requirement_in_near(UPVOTE_KUDOS_COST)
        );

        let external_db_id = self.external_db_id()?.clone();
        let gas_remaining =
            env::prepaid_gas() - (env::used_gas() + IS_HUMAN_GAS + UPVOTE_KUDOS_RESERVED_GAS);

        Ok(ext_sbtreg::ext(self.iah_registry.clone())
            .with_static_gas(IS_HUMAN_GAS)
            .is_human(env::signer_account_id())
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(gas_remaining)
                    .acquire_kudos_sender(
                        predecessor_account_id,
                        attached_deposit.into(),
                        external_db_id,
                        receiver_id,
                        kudos_id,
                    ),
            ))
    }

    /// Allows caller to give kudos for a user by [`AccountId`].
    /// Caller should have a valid i-am-human SBT and can't give kudos to himself.
    /// Hashtags is an array of [`String`] for which only alphanumeric characters, underscores and gyphens are allowed to use.
    #[payable]
    #[handle_result]
    pub fn give_kudos(
        &mut self,
        receiver_id: AccountId,
        kind: Option<KudosKind>,
        message: String,
        icon_cid: Option<WrappedCid>,
        hashtags: Option<Vec<String>>,
    ) -> Result<Promise, &'static str> {
        self.assert_contract_running();

        let predecessor_account_id = env::predecessor_account_id();
        let sender_id = env::signer_account_id();
        require!(
            receiver_id != sender_id,
            "User is not eligible to upvote this kudos"
        );

        let minimum_gas_requirement = GIVE_KUDOS_RESERVED_GAS
            + IS_HUMAN_GAS
            + SAVE_KUDOS_RESERVED_GAS
            + SOCIAL_DB_REQUEST_MIN_RESERVED_GAS
            + KUDOS_SAVED_CALLBACK_GAS
            + FAILURE_CALLBACK_GAS;
        require!(
            env::prepaid_gas() >= minimum_gas_requirement,
            display_gas_requirement_in_tgas(minimum_gas_requirement)
        );

        let attached_deposit = env::attached_deposit();
        require!(
            attached_deposit == GIVE_KUDOS_COST,
            &display_deposit_requirement_in_near(GIVE_KUDOS_COST)
        );

        let settings = Settings::from(&self.settings);
        let kind = kind.unwrap_or_default();
        let hashtags = settings.validate_hashtags(hashtags.as_deref())?;
        if message.len() > Settings::from(&self.settings).commentary_message_max_length  as usize {
            return Err("Message max length exceeded");
        }

        let external_db_id = self.external_db_id()?.clone();

        let gas_remaining =
            env::prepaid_gas() - (env::used_gas() + IS_HUMAN_GAS + GIVE_KUDOS_RESERVED_GAS);

        Ok(ext_sbtreg::ext(self.iah_registry.clone())
            .with_static_gas(IS_HUMAN_GAS)
            .is_human(sender_id)
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(gas_remaining)
                    .save_kudos(
                        predecessor_account_id,
                        attached_deposit.into(),
                        external_db_id,
                        receiver_id,
                        kind,
                        message,
                        icon_cid,
                        hashtags,
                    ),
            ))
    }
}
