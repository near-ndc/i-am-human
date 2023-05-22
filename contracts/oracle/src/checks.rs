#[cfg(all(test, not(target_arch = "wasm32")))]
pub mod tests {

    use crate::*;
    pub fn deserialize_claim(claim_b64: &str) -> Claim {
        let c_bz = crate::b64_decode("claim", claim_b64.to_string()).unwrap();
        let c = Claim::try_from_slice(&c_bz).unwrap();
        println!("claim: {:?}", c);
        c
    }

    #[test]
    fn claim_deserialization_check() {
        let c = deserialize_claim("EQAAAGhhcnJ5ZGhpbGxvbi5uZWFyKgAAADB4YjRiZjBmMjNjNzAyZWZiOGE5ZGE4N2E5NDA5NWUyOGRlM2QyMWNjMyDzAGQAAAAA");
        assert_eq!(
            c.external_id, "0xb4bf0f23c702efb8a9da87a94095e28de3d21cc3",
            "deserialization check"
        );
    }

    #[test]
    fn check_claim_hostess() {}
}
