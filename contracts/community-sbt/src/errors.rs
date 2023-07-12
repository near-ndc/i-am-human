use near_sdk::env::panic_str;
use near_sdk::FunctionError;

/// Contract errors
#[cfg_attr(not(target_arch = "wasm32"), derive(PartialEq))]
#[derive(Debug)]
pub enum MintError {
    NotMinter,
    RequiredDeposit(u128),
    ClassNotEnabled,
}

impl FunctionError for MintError {
    fn panic(&self) -> ! {
        match self {
            MintError::NotMinter => panic_str("not authorized to mint"),
            MintError::RequiredDeposit(min_deposit) => {
                panic_str(&format!("deposit must be at least {}yN", min_deposit))
            }
            MintError::ClassNotEnabled => panic_str("class not enabled"),
        }
    }
}
