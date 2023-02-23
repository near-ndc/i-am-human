use near_sdk::env::panic_str;
use near_sdk::{base64, FunctionError};

/// Contract errors
pub enum CtrError {
    Borsh(String),
    B64Err {
        /// name of the argument being decoded
        arg: String,
        err: base64::DecodeError,
    },
    BadRequest(String),
    DuplicatedID(String),
}

impl FunctionError for CtrError {
    fn panic(&self) -> ! {
        // check how we can do this:
        // panic_str(match self {
        //     CtrError::Borsh(e) => &format!("can't borsh decode {}", e),
        //     CtrError::B64Err { arg, .. } => &format!("can't base64 decode {}", arg),
        //     CtrError::BadRequest(s) => s.as_ref(),
        // });

        match self {
            CtrError::Borsh(e) => panic_str(&format!("can't borsh-decode {}", e)),
            CtrError::B64Err { arg, .. } => panic_str(&format!("can't base64-decode {}", arg)),
            CtrError::BadRequest(s) => panic_str(s.as_ref()),
            CtrError::DuplicatedID(s) => panic_str(&format!("duplicated id: {}", s)),
        }
    }
}
