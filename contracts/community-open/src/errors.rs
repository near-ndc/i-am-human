use near_sdk::env::panic_str;
use near_sdk::FunctionError;

/// Contract errors
#[cfg_attr(not(target_arch = "wasm32"), derive(PartialEq, Debug))]
pub enum Error {
    NotAdmin,
    NotMinter,
    RequiredDeposit(u128),
    ClassNotFound,
}

impl FunctionError for Error {
    fn panic(&self) -> ! {
        match self {
            Error::NotAdmin => panic_str("not authorized: required admin"),
            Error::NotMinter => panic_str("not authorized: required minter"),
            Error::RequiredDeposit(min_deposit) => {
                panic_str(&format!("deposit must be at least {}yN", min_deposit))
            }
            Error::ClassNotFound => panic_str("class not found"),
        }
    }
}
