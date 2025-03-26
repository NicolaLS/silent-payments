# BOSS POC: Silent Payment indexer

- Store and serve tweak data.
- Store and serve taproot output transactions.
- Given a mnemonic code, detect outputs to silent payment address.

# API

note: this is just brainstorming, no idea if this is going to look like this in the end.

GET /blocks/height/<height>/transactions
GET /blocks/height/<height>/tweaks

GET /transactions/<txid>
GET /transactions/<txid>/tweak

POST /wallet (create wallet and return wallet id or mnemonic) (only use for development)

Websocket subscriptions:
/ws/tweaks: Stream new tweaks.
/ws/transactions: Stream new transactions.
/ws/<wallet_id>/outputs: Stream outputs owned by wallet

# DB

Since this is a POC, I'll use Sqlite but ofc. for a real project Postgres would be better. I never
worked much with DB's so I'm having some problems.
- Sqlite has max integer i64 but many types I have are usize or u64...right now just casting and panic
if something goes wrong but I guess I'll have to store it as BLOB instead.
- Sqlite does not have structured types, so storing tx. data is annoying.

I won't store all tx. data, we are actually only (at most) interested in:
- txid
- output (vout, scriptPubKey, value)
- tweaks

Because it is weird to store a Vec/List of outputs in sqlite I just have another table of outputs
with forign key referencing txs.

Tables (for now):

**Blocks**
- height INTEGER (PK)
- hash STRING (hex)
- tx_count INTEGER

**Transactions**
- id INTEGER (PK) (not txid)
- block INTEGER (references block(height))
- txid STRING (hex)
- scalar STRING (hex)

**Outputs**
- id INTEGER (PK)
- tx INTEGER (references transactions(id), used for outpoint)
- vout INTEGER (outpoint index)
- value INTEGER (sats)
- script_pub_key STRING (hex encoded)

# Plan

## JSON-RPC vs. gRPC vs REST

Initially I wanted to use JSON-RPC/gRPC because I thought its a good fit for subscriptions but it
actually does not make any sense because the main purpose is just to GET public tweak data.

So I'll implement a REST API with `axum` and for the subscriptions just have simple websockets that
only stream data but take no requests.

## Functionality

Look at API for reference.
- get public tweak data for blocks, or transactions.
- get transactions by block or txid
- create a wallet (getting a unique wallet id)
- subscribe to new public tweaks via WS
- subscribe to new txs of new blocks via WS
- subscribe to outpouts for some `id` wallet via WS.

Wallet should ofc. only be used for demos/testing.


# Notes

## Partial tweak

Silent Payment tweak is:
```
input_hash = hash(outpoint_L || A)

shared_secret_alice = input_hash * a * B_scan
shared_secret_bob = input_hash * A * b_scan

tweak = hash(shared_secret || 0) * G
```

**Question:** why do we concat `0` ???

It's called the tweak because it is added to `B_spend` by alice, so only Bob can spend the coins.
We can only calculate the tweak with `b_spend` so we'll need to do as much as possible if we don't
have it, which is not much..:

```
shared_secret_bob_partial = input_hash * A
```

## Labels

I was a bit confused about the labels but I think I understand it now.

See: [labels](https://github.com/bitcoin/bips/blob/master/bip-0352.mediawiki#overview) and [scanning](https://github.com/bitcoin/bips/blob/master/bip-0352.mediawiki#scanning) from BIP352.

The label is actually like a second _tweak_ that is done by the receiver before giving out the
silent payment address. The nice thing is that `B_scan` stays the same so scanning remains easy but
`B_spend` becomes `B_m` to distinguish with labels.

So now the actual public key that is finally used is `B_spend + tweak + label` (so the label is
basically another tweak but only created by the receiver). This is because we change `B_spend`
to be `Bm = B_spend + label`.

-> Sender does not have to do anything special. He used `B_scan` and `B_spend` like before but
   now he actually sees `B_scan` and `B_m`.

Scanning confused me a bit because it feels a bit reversed because of the label. When scanning we just
check the output as if we never added the label tweak to our spend key. So `P = B_spend + tweak`.

Remember here, that we are trying to find a matching output. An output will look like this:
`output = B_m + tweak` which is `output = B_spend + label + tweak`.

So when scanning without considering the label we don't find a matching output, we subtract `P` from
the output:
```
label = output - P
label = (B_m + tweak) - (B_spend + tweak)
label = (B_spend + label + tweak) - (B_spend + tweak)
label = label
```

If this `label` is used in the wallet add the output + label to the wallet. You can spend it
with `b_spend + label + tweak`.


**Question:** Why does it even say to check the output before subtracting in the BIP? Does
that mean "labels" is opt in feature and we can use no labels and only label 0 for change if we
want? I thought you'd have to use at least "1" label.


# Logging

To ignore `debug` logs from external crates (e.g. `sqlx`) run:
`RUST_LOG=silent_payments_server=debug cargo run`

To see all logs on that log level just do:
`RUST_LOG=debug cargo run`
