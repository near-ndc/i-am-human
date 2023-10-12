use crate::external_db::ext_db;
use crate::misc::RunningState;
use crate::settings::{Settings, SettingsView, VSettings};
use crate::types::{KudosId, StorageKey};
use crate::utils::build_initial_json_for_socialdb;
use crate::IncrementalUniqueId;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::store::LookupSet;
use near_sdk::{
    env, near_bindgen, require, AccountId, Balance, PanicOnDefault, Promise, PromiseError,
    ONE_YOCTO,
};

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    /// A valid [`AccountId`] which represents a contract's owner/admin
    pub(crate) owner_id: AccountId,
    /// Contract's state [`RunningState`], e.g. running, paused
    pub(crate) running_state: RunningState,
    /// Last unique incremenetal identifier [`IncrementalUniqueId`] used to get next incremented unique [`KudosId`] and [`CommentId`]
    pub(crate) last_incremental_id: IncrementalUniqueId,
    /// A valid [`AccountId`] of NEAR social db smart contract, should be set by calling `set_external_db` method.
    /// Used to store and read saved kudos information.
    pub(crate) external_db_id: Option<AccountId>,
    /// A valid [`AccountId`] of i-am-human-registry smart contract, set during a contract initialization.
    /// Used to check for humanity and to exchange upvoted kudos for a ProofOfKudos SBT
    pub(crate) iah_registry: AccountId,
    /// Upgradable [`VSettings`] for this smart contract, which represents some configurable settings,
    /// e.g. max commentary message length, etc.
    pub(crate) settings: VSettings,
    /// [`LookupSet`] of unique [`KudosId`] to memorise exchanged kudos for ProofOfKudos SBT.
    /// Used to guarantee upvotes kudos to be exchanged only once.
    pub(crate) exchanged_kudos: LookupSet<KudosId>,
}

#[near_bindgen]
impl Contract {
    /// Initializes contract with default values, allows to set a valid [`AccountId`] as an owner of a contract initially.
    /// Requires a valid [`AccountId`] for i-am-human-registry smart contract.
    #[init]
    pub fn init(owner_id: Option<AccountId>, iah_registry: AccountId) -> Self {
        Self {
            owner_id: owner_id.unwrap_or_else(env::predecessor_account_id),
            running_state: RunningState::Running,
            last_incremental_id: IncrementalUniqueId::default(),
            external_db_id: None,
            iah_registry,
            settings: Settings::default().into(),
            exchanged_kudos: LookupSet::new(StorageKey::Kudos),
        }
    }

    /// Replaces [`AccountId`] of i-am-human-registry smart contract which is used to verify humanity and
    /// to exchange kudos for ProofOfKudos SBT. Restricted to be used only by an owner/admin of this contract.
    #[payable]
    #[handle_result]
    pub fn update_iah_registry(&mut self, iah_registry: AccountId) -> Result<Promise, String> {
        self.assert_owner();

        let external_db_id = self.external_db_id()?.clone();

        self.grant_write_permission(external_db_id, iah_registry, env::attached_deposit())
    }

    /// Sets [`AccoundId`] of NEAR social db smart contract as an external storage for kudos.
    /// Restricted to be used only by an owner/admin of this contract.
    #[payable]
    #[handle_result]
    pub fn set_external_db(&mut self, external_db_id: AccountId) -> Result<Promise, &'static str> {
        self.assert_owner();
        // Do not allow to change SocialDB address, because there is no data migration possible.
        // It should be initialized only once.
        require!(
            self.external_db_id.is_none(),
            "External database already set"
        );

        let root_id = env::current_account_id();
        let initial_json = build_initial_json_for_socialdb(&root_id)?;

        // This will require a minimum amount of deposit to register a user for Kudos contract.
        // Minimum amount of deposit required could be priorly acquired by calling a view method
        // `storage_balance_bounds` to Social-Db contract and one yocto additionally should be added
        // to request `grant_write_permission` method
        Ok(ext_db::ext(external_db_id.clone())
            .with_attached_deposit(env::attached_deposit() - ONE_YOCTO)
            .set(initial_json)
            .then(Self::ext(env::current_account_id()).on_ext_db_init(external_db_id, ONE_YOCTO)))
    }

    /// Public view method to read current settings [`SettingsView`] of this contract
    pub fn view_settings(&self) -> SettingsView {
        Settings::from(&self.settings).into()
    }

    /// Updates specified settings [`SettingsView`] for this smart contract.
    /// Restricted to be used only by an owner/admin of this contract.
    #[payable]
    pub fn update_settings(&mut self, settings_json: SettingsView) {
        self.assert_owner();

        self.settings = self.settings.apply_changes(settings_json);
    }

    /// Internal helper method to grant write permission to IAH Registry
    ///
    /// Write permissions required to be able to use `IAHRegistry::is_human_call`, because SocialDB checks for a predecessor_id.
    ///
    /// ATTENTION: `is_human_call` is not being used by this smart contract right now, but we still want to be able
    /// to use it in future, so the code below is necessary.
    fn grant_write_permission(
        &mut self,
        external_db_id: AccountId,
        iah_registry: AccountId,
        deposit: Balance,
    ) -> Result<Promise, String> {
        Ok(ext_db::ext(external_db_id.clone())
            .with_attached_deposit(deposit)
            .grant_write_permission(
                Some(iah_registry.clone()),
                None,
                vec![format!("{}", env::current_account_id())],
            )
            .then(
                Self::ext(env::current_account_id())
                    .on_ext_db_write_permission_granted(external_db_id, iah_registry),
            ))
    }

    #[private]
    #[handle_result]
    pub fn on_ext_db_init(
        &mut self,
        external_db_id: AccountId,
        deposit: Balance,
        #[callback_result] callback_result: Result<(), PromiseError>,
    ) -> Result<Promise, String> {
        callback_result.map_err(|e| format!("SocialDB::set() call failure: {:?}", e))?;

        self.grant_write_permission(external_db_id, self.iah_registry.clone(), deposit)
    }

    #[private]
    #[handle_result]
    pub fn on_ext_db_write_permission_granted(
        &mut self,
        external_db_id: AccountId,
        iah_registry: AccountId,
        #[callback_result] callback_result: Result<(), PromiseError>,
    ) -> Result<(), String> {
        callback_result.map_err(|e| {
            format!(
                "SocialDB::grant_write_permission(`{iah_registry}`) call failure: {:?}",
                e
            )
        })?;

        self.iah_registry = iah_registry;
        self.external_db_id = Some(external_db_id);

        Ok(())
    }
}

impl Contract {
    /// Check and panic if contract state [`RunningState`] is not set to [`RunningState::Running`]
    pub(crate) fn assert_contract_running(&self) {
        require!(
            self.running_state == RunningState::Running,
            "Contract paused"
        );
    }

    /// Asserts if the caller is not an owner/admin of this contract
    pub(crate) fn assert_owner(&self) {
        require!(self.is_owner(&env::predecessor_account_id()), "Not allowed");
    }

    /// Return [`bool`] which represents if [`AccountId`] is an owner/admin of this contract or not
    pub(crate) fn is_owner(&self, account_id: &AccountId) -> bool {
        account_id == &self.owner_id
    }

    /// Return [`AccountId`] of NEAR social db smart contract used by this contract or an error if not set
    pub(crate) fn external_db_id(&self) -> Result<&AccountId, &'static str> {
        self.external_db_id.as_ref().ok_or("External db is not set")
    }
}
