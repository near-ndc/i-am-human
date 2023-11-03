use near_sdk::env::panic_str;
use near_sdk::FunctionError;

#[cfg_attr(not(target_arch = "wasm32"), derive(PartialEq, Debug))]
pub enum IsHumanCallErr {
    NotHuman,
}

impl FunctionError for IsHumanCallErr {
    fn panic(&self) -> ! {
        match self {
            IsHumanCallErr::NotHuman => panic_str("caller is not a human"),
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), derive(PartialEq, Debug))]
pub enum SoulTransferErr {
    TransferLocked,
}

impl FunctionError for SoulTransferErr {
    fn panic(&self) -> ! {
        match self {
            SoulTransferErr::TransferLocked => {
                panic_str("soul transfer not possible: owner has a transfer lock")
            }
        }
    }
}
