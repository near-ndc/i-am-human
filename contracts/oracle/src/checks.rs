#[cfg(all(test, not(target_arch = "wasm32")))]
pub mod tests {

    use crate::*;
    pub fn deserialize_claim(claim_b64: &str) -> Claim {
        let c_bz = crate::b64_decode("claim", claim_b64.to_string()).unwrap();
        let c = Claim::try_from_slice(&c_bz).unwrap();
        println!("claim: {:?}", c);
        c
    }

    fn alice() -> AccountId {
        AccountId::new_unchecked("alice.near".to_string())
    }

    #[test]
    fn borsh_simple() {
        let borsh_input = Claim {
            claimer: alice(),
            external_id: "0xb4bf0f23c702efb8a9da87a94095e28de3d21cc3".to_owned(),
            timestamp: 0,
            verified_kyc: false,
        };

        let borsh_serialized: Vec<u8> = borsh_input.try_to_vec().unwrap();
        let base64_encoded = near_primitives::serialize::to_base64(borsh_serialized.as_slice());
        println!(
            "Using NEAR CLI, this is the base64-encoded value to use: {:?}",
            base64_encoded
        );
    }

    #[test]
    fn claim_deserialization_check() {
        let c = deserialize_claim("CgAAAGFsaWNlLm5lYXIqAAAAMHhiNGJmMGYyM2M3MDJlZmI4YTlkYTg3YTk0MDk1ZTI4ZGUzZDIxY2MzAAAAAAAAAAAA");
        assert_eq!(
            c.external_id, "0xb4bf0f23c702efb8a9da87a94095e28de3d21cc3",
            "deserialization check"
        );
    }

    #[test]
    fn check_claim_hostess() {}
}
