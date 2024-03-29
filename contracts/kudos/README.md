# Kudos

Kudos contract for NDC

## Setup [Testnet]

- Init contract

  ```sh
  near call $CONRTACT_ID init '{"iah_registry": "registry-unstable.i-am-human.testnet"}' --accountId YOUR_ACCOUNT.testnet
  near call $CONRTACT_ID set_external_db '{"external_db_id": "v1.social08.testnet"}' --accountId YOUR_ACCOUNT.testnet --amount 5
  ```

- Deploy it on testnet

  ```sh
  near dev-deploy target/wasm32-unknown-unknown/release/kudos_contract.wasm
  ```

## Public methods

### Give kudos

Enables the caller to award kudos to a recipient's account.

#### Requirements

User must be human verified (should have a valid i-am-human SBT)
Minimum gas required: 67 TGas (300 TGas recommended)
Deposit required: 0.1 Ⓝ

#### Interface

```js
give_kudos(receiver_id, message, icon_cid, hashtags): kudos id

- receiver_id: user's account id that should be awarded the kudos
- message: comment message on the kudos. Max character limit is 1000
- icon_cid: optional valid CID for icon (image) at ipfs
- hashtags: optional array of user-specified tags (limited to 32 characters by default, and allows to use only alphanumeric characters, underscores and gyphens). By default maximum allowed number of hashtags is 10
```

#### Output

Returns unique kudos identifier or panics with an error message

Example JSON written to SocialDB:

```js
{
  "kudos.near": {
    "kudos": {
      "some_user.near": {
        "1": {
          "created_at": "1689976833613",
          "sender_id": "alex.near",
          "kind": "k",
          "message": "that user is awesome",
          "icon": "bafybeigrf2dwtpjkiovnigysyto3d55opf6qkdikx6d65onrqnfzwgdkfa",
          "upvotes": {},
          "comments": {},
          "tags": "[\"firstkudos\",\"awesomework\"]",
        }
      }
    },
    "hashtags": {
      "firstkudos": {
        "1": "alex.near"
      },
      "awesomework": {
        "1": "alex.near"
      }
    }
  }
}
```

### Upvote kudos

Allows caller to upvote kudos by unique id granted to a receiver account

#### Requirements

- User must be human verified (should have a valid i-am-human SBT)
- Caller can't be a account which granted kudos
- Caller can't be a receiver account
- Caller could upvote specified kudos only once
- Minimum gas required: 92 TGas (300 TGas recommended)
- Deposit required: 0.004 Ⓝ

#### Interface

```js
upvote_kudos(receiver_id, kudos_id): timestamp

- receiver_id: user's account id whos unique kudos should be upvoted
- kudos_id: unique kudos identified granted to a receiver account
```

#### Output

Returns stringified timestamp of block when kudos was upvoted or panics with an error message

Example JSON written to SocialDB:

```js
{
  "kudos.near": {
    "kudos": {
      "some_user.near": {
        "1": {
          "upvotes": {
            "bob.near": ""
          }
        }
      }
    }
  }
}
```

### Leave commentary message to kudos

Allows caller to leave a commentary message to kudos by unique id granted to a receiver account

#### Requirements

- User must be human verified (should have a valid i-am-human SBT)
- User can't leave a comment for his kudos, but it can reply to other comments
- Minimum gas required: 92 TGas (300 TGas recommended)
- Deposit required: 0.017 Ⓝ

#### Interface

```js
leave_comment(receiver_id, kudos_id, parent_comment_id, message): commentary id

- receiver_id: user's NEAR account id whos unique kudos should be upvoted
- kudos_id: unique kudos identified granted to a receiver NEAR account
- parent_comment_id: optional parent commentary id which this new comment is a reply for. By default, if not specified, every commentary relates to kudos id
- message: followed commentary message text to the kudos. By default limits to 1000 characters
```

#### Output

Returns unique commentary identifier or panics with an error message

Example JSON written to SocialDB:

```js
{
  "kudos.near": {
    "kudos": {
      "some_user.near": {
        "1": {
          "comments": {
            "2": "eyJtIjoiY29tbWVudGFyeSB0ZXN0IiwicyI6InVzZXIubmVhciIsInQiOiIxMjM0NTY3ODkwIn0="
          }
        }
      }
    }
  }
}
```

### Exchange upvoted kudos for ProofOfKudos SBT

Allows caller to exchange his upvoted kudos by unique id for a ProofOfKudos SBT

#### Requirements

- User must be human verified (should have minted and valid i-am-human SBT)
- Caller should be granted with kudos by provided unique identifier
- Caller can exchange his upvoted kudos only once
- Kudos should have minimum required number of upvotes. By default is 3 upvotes
- Minimum gas required: 87 TGas (300 TGas recommended)
- Deposit required: 0.008 Ⓝ

#### Interface

```js
exchange_kudos_for_sbt(kudos_id): array of minted SBTs

- kudos_id: unique kudos identified granted to a caller account
```

#### Output

Returns an array of minted ProofOfKudos SBTs in exchange for kudos or panics with an error message
