use anyhow::anyhow;
use kudos_contract::registry::{OwnedToken, TokenMetadata};
use kudos_contract::{
    CommentId, KudosId, WrappedCid, EXCHANGE_KUDOS_COST, GIVE_KUDOS_COST, LEAVE_COMMENT_COST,
    PROOF_OF_KUDOS_SBT_MINT_COST, SOCIAL_DB_GRANT_WRITE_PERMISSION_COST, UPVOTE_KUDOS_COST,
};
use near_contract_standards::storage_management::{StorageBalance, StorageBalanceBounds};
use near_sdk::json_types::U64;
use near_sdk::serde_json::json;
use near_sdk::{AccountId, ONE_YOCTO};
use workspaces::result::ExecutionOutcome;

pub async fn mint_fv_sbt(
    iah_registry_id: &workspaces::AccountId,
    issuer: &workspaces::Account,
    receivers: &[&workspaces::AccountId],
    issued_at: u64,  // SBT issued at in millis
    expires_at: u64, // SBT expires at in millis
) -> anyhow::Result<Vec<u64>> {
    let res = issuer
        .call(iah_registry_id, "sbt_mint")
        .args_json(json!({
          "token_spec": receivers.iter().map(|receiver_id| (receiver_id, [
              TokenMetadata {
                  class: 1, // FV SBT
                  issued_at: Some(issued_at),
                  expires_at: Some(expires_at),
                  reference: None,
                  reference_hash: None,
              }
            ])
            ).collect::<Vec<_>>()
        }))
        .deposit(PROOF_OF_KUDOS_SBT_MINT_COST * receivers.len() as u128)
        .max_gas()
        .transact()
        .await?
        .into_result()
        .map_err(|e| {
            anyhow::Error::msg(format!(
                "Mint FV SBT failure: {:?}",
                extract_error(e.outcomes().into_iter())
            ))
        });

    res.and_then(|res| {
        println!("gas burnt: {}", res.total_gas_burnt);
        res.json().map_err(|e| {
            anyhow::Error::msg(format!(
                "Failed to deserialize sbt_mint response: {e:?}. Receipts: {:?}",
                res.receipt_outcomes()
            ))
        })
    })
}

pub async fn verify_is_human(
    iah_registry_id: &workspaces::AccountId,
    issuer_id: &workspaces::AccountId,
    users_accounts: &[&workspaces::Account],
    tokens: &Vec<u64>,
) -> anyhow::Result<()> {
    for (i, &user_account) in users_accounts.iter().enumerate() {
        let res = user_account
            .view(iah_registry_id, "is_human")
            .args_json(json!({
              "account": user_account.id()
            }))
            .await?
            .json::<Vec<(AccountId, Vec<u64>)>>()?;

        match res.first() {
            Some((issuer_id_result, tokens_result))
                if issuer_id_result.as_str() != issuer_id.as_str()
                    && tokens_result[0] != tokens[i] =>
            {
                return Err(anyhow::Error::msg(format!(
                    "User `{}` not verified",
                    user_account.id()
                )));
            }
            _ => (),
        };
    }

    Ok(())
}

pub async fn verify_kudos_sbt_tokens_by_owner(
    iah_registry_id: &workspaces::AccountId,
    issuer_id: &workspaces::AccountId,
    owner: &workspaces::Account,
    tokens_ids: &[u64],
) -> anyhow::Result<()> {
    let res = owner
        .view(iah_registry_id, "sbt_tokens_by_owner")
        .args_json(json!({
          "account": owner.id(),
          "issuer": issuer_id,
        }))
        .await?
        .json::<Vec<(AccountId, Vec<OwnedToken>)>>()?;

    match res.first() {
        Some((issuer_id_result, tokens_result))
            if issuer_id_result.as_str() != issuer_id.as_str()
                && compare_slices(
                    &tokens_result
                        .iter()
                        .map(|token_res| token_res.token)
                        .collect::<Vec<_>>(),
                    tokens_ids,
                ) =>
        {
            Err(anyhow::Error::msg(format!(
                "User `{}` do not have ProofOfKudos SBT",
                owner.id()
            )))
        }
        _ => Ok(()),
    }
}

pub async fn give_kudos(
    kudos_contract_id: &workspaces::AccountId,
    sender: &workspaces::Account,
    receiver_id: &workspaces::AccountId,
    message: &str,
    icon_cid: Option<&WrappedCid>,
    hashtags: Vec<&str>,
) -> anyhow::Result<KudosId> {
    let res = sender
        .call(kudos_contract_id, "give_kudos")
        .args_json(json!({
            "receiver_id": receiver_id,
            "message": message,
            "hashtags": hashtags,
            "icon_cid": icon_cid
        }))
        .deposit(GIVE_KUDOS_COST)
        .max_gas()
        .transact()
        .await?
        .into_result()
        .map_err(|e| {
            anyhow::Error::msg(format!(
                "Give kudos failure: {:?}",
                extract_error(e.outcomes().into_iter())
            ))
        });

    res.and_then(|res| {
        println!("gas burnt: {}", res.total_gas_burnt);
        res.json().map_err(|e| {
            anyhow::Error::msg(format!(
                "Failed to deserialize give kudos response: {e:?}. Receipts: {:?}",
                res.receipt_outcomes()
            ))
        })
    })
}

pub async fn upvote_kudos(
    kudos_contract_id: &workspaces::AccountId,
    sender: &workspaces::Account,
    receiver_id: &workspaces::AccountId,
    kudos_id: &KudosId,
) -> anyhow::Result<U64> {
    let res = sender
        .call(kudos_contract_id, "upvote_kudos")
        .args_json(json!({
            "receiver_id": receiver_id,
            "kudos_id": kudos_id,
        }))
        .deposit(UPVOTE_KUDOS_COST)
        .max_gas()
        .transact()
        .await?
        .into_result()
        .map_err(|e| {
            anyhow::Error::msg(format!(
                "Upvote kudos failure: {:?}",
                extract_error(e.outcomes().into_iter())
            ))
        });

    res.and_then(|res| {
        println!("gas burnt: {}", res.total_gas_burnt);
        res.json().map_err(|e| {
            anyhow::Error::msg(format!(
                "Failed to deserialize upvote kudos response: {e:?}. Receipts: {:?}",
                res.receipt_outcomes()
            ))
        })
    })
}

pub async fn leave_comment(
    kudos_contract_id: &workspaces::AccountId,
    sender: &workspaces::Account,
    receiver_id: &workspaces::AccountId,
    kudos_id: &KudosId,
    parent_comment_id: Option<CommentId>,
    message: &str,
) -> anyhow::Result<CommentId> {
    let res = sender
        .call(kudos_contract_id, "leave_comment")
        .args_json(json!({
            "receiver_id": receiver_id,
            "kudos_id": kudos_id,
            "parent_comment_id": parent_comment_id,
            "message": message,
        }))
        .deposit(LEAVE_COMMENT_COST)
        .max_gas()
        .transact()
        .await?
        .into_result()
        .map_err(|e| {
            anyhow::Error::msg(format!(
                "Leave comment failure: {:?}",
                extract_error(e.outcomes().into_iter())
            ))
        });

    res.and_then(|res| {
        println!("gas burnt: {}", res.total_gas_burnt);
        res.json().map_err(|e| {
            anyhow::Error::msg(format!(
                "Failed to deserialize leave comment response: {e:?}. Receipts: {:?}",
                res.receipt_outcomes()
            ))
        })
    })
}

pub async fn exchange_kudos_for_sbt(
    kudos_contract_id: &workspaces::AccountId,
    requestor: &workspaces::Account,
    kudos_id: &KudosId,
) -> anyhow::Result<Vec<u64>> {
    let res = requestor
        .call(kudos_contract_id, "exchange_kudos_for_sbt")
        .args_json(json!({
            "kudos_id": kudos_id,
        }))
        .deposit(EXCHANGE_KUDOS_COST)
        .max_gas()
        .transact()
        .await?
        .into_result()
        .map_err(|e| {
            anyhow::Error::msg(format!(
                "Exchange kudos failure: {:?}",
                extract_error(e.outcomes().into_iter())
            ))
        });

    res.and_then(|res| {
        res.json().map_err(|e| {
            anyhow::Error::msg(format!(
                "Failed to deserialize exchange kudos response: {e:?}. Receipts: {:?}",
                res.receipt_outcomes()
            ))
        })
    })
}

pub async fn set_external_db(
    kudos_contract_id: &workspaces::AccountId,
    owner: &workspaces::Account,
    near_social: &workspaces::Contract,
) -> anyhow::Result<()> {
    let balance_bounds: StorageBalanceBounds = near_social
        .view("storage_balance_bounds")
        .args_json(json!({}))
        .await?
        .json()?;

    let _ = owner
        .call(kudos_contract_id, "set_external_db")
        .args_json(json!({
            "external_db_id": near_social.id()
        }))
        .deposit(balance_bounds.min.0 + ONE_YOCTO)
        .max_gas()
        .transact()
        .await?
        .into_result()
        .map_err(|e| {
            anyhow::Error::msg(format!(
                "Set external database failure: {:?}",
                extract_error(e.outcomes().into_iter())
            ))
        })?;

    Ok(())
}

pub async fn update_iah_registry(
    kudos_contract_id: &workspaces::AccountId,
    owner: &workspaces::Account,
    iah_registry: &workspaces::AccountId,
) -> anyhow::Result<()> {
    let _ = owner
        .call(kudos_contract_id, "update_iah_registry")
        .args_json(json!({ "iah_registry": iah_registry }))
        .deposit(SOCIAL_DB_GRANT_WRITE_PERMISSION_COST)
        .max_gas()
        .transact()
        .await?
        .into_result()
        .map_err(|e| {
            anyhow::Error::msg(format!(
                "Update IAH registry failure: {:?}",
                extract_error(e.outcomes().into_iter())
            ))
        })?;

    Ok(())
}

pub async fn storage_balance_of(
    contract_id: &workspaces::AccountId,
    user: &workspaces::Account,
) -> anyhow::Result<Option<StorageBalance>> {
    user.view(contract_id, "storage_balance_of")
        .args_json(json!({
          "account_id": user.id()
        }))
        .await?
        .json()
        .map_err(|e| {
            anyhow::Error::msg(format!("Storage balance of `{}` failure: {e:?}", user.id(),))
        })
}

// TODO: pass iterators instead
fn compare_slices<T: PartialEq>(sl1: &[T], sl2: &[T]) -> bool {
    let count = sl1
        .iter()
        .zip(sl2)
        .filter(|&(item1, item2)| item1 == item2)
        .count();

    count == sl1.len() && count == sl2.len()
}

pub fn extract_error<'a, I>(mut outcomes: I) -> anyhow::Error
where
    I: Iterator<Item = &'a ExecutionOutcome>,
{
    outcomes
        .find(|&outcome| outcome.is_failure())
        //.and_then(|outcome| outcome.clone().into_result().err())
        .map(|outcome| {
            outcome
                .clone()
                .into_result()
                .map_err(|e| anyhow!(e.into_inner().unwrap()))
                .unwrap_err()
        })
        .unwrap()
}
