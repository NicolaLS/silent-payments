# Chaincode BOSS POC: Silent Payment Indexer

- Store and serve tweak data.
- Store and serve taproot output transactions.
- Given a mnemonic code, detect outputs to silent payment address.

## Configuration

Server is configured with env. variables for development a `.env` file can be used. No defaults are
provided, and the server will error if any variable is missing. See [.env](./server/.env).

## Running the server

SQLite database is created if it does not exist and migrations are also automatically run.
The server will start syncing from the confiugred `SYNC_FROM` height.

**Run server**
`cargo run`

**Run tests**
`cargo test`

## REST API

`GET /blocks/tip`

_Returns current synced block height._

`GET /blocks/latest/scalars`

_Returns scalars in the latest synced block_

```json
{
  "scalars": [
    "0300260cd166b0b9375963fdeea829c638ad74e69ddba80a43bf3388619d2ee96d",
    "02393c02d8fce020e37e709367a74835bc2f4a292307be15d34211fe6982494caf",
    "025e39ed89ccaf2e0d654f1540a427063e4fb7087526529e163729830dce4ffc52",
    "03f6d1cb5ea84ec62d82a42c1ee55d9da96457a9823b9067981b04a7fc99df623b",
  ]
}
```

`GET /blocks/latest/transactions`

_Returns all transactions in this block. Transactions are only BIP-352 eligible transactions and do
not contain most bitcoin transaction data but the data that is useful to wallets, which is txid, scalar
and outputs (vout, value, spk hex)._

```json
{
  "transactions": [
    {
      "txid": "370818bea6e50a63d628d6fa179411237be5a45419a2c36867926e50b48ca848",
      "scalar": "035c2fb8ce078f77db70beb7317dede4cd079a83fc231c3c34d222faa306e7c48c",
      "outputs": [
        {
          "vout": 0,
          "value": 988438,
          "spk": "5120ae66becf5234528a3f9d3e64545066a42f55c625daf288827c96fc5757c10c2b"
        },
        {
          "vout": 1,
          "value": 100000000,
          "spk": "5120cc685d57c383b48ec9bbce71668ecda8c90aa57c5012347557484dfbcfff8981"
        }
      ]
    },
    {
      "txid": "98649f70b9a5b4c6ab78b2fe1f43eb09b3c8218bccb914dacfc1d6a18991d035",
      "scalar": "025e39ed89ccaf2e0d654f1540a427063e4fb7087526529e163729830dce4ffc52",
      "outputs": [
        {
          "vout": 0,
          "value": 100000000,
          "spk": "5120cc685d57c383b48ec9bbce71668ecda8c90aa57c5012347557484dfbcfff8981"
        },
        {
          "vout": 1,
          "value": 430221,
          "spk": "51204e0fcc0220dc1a0e26ce0960e1fa6c7f73d1e2ebb1813d2a787fab95c17aed13"
        }
      ]
    }
  ]
}
```

`GET /blocks/height/<height>/scalars`

_Returns the scalars for thsi block height. Same response format as `/blocks/latest/scalars`._

`GET /blocks/height/<height>/transactions`

_Returns the transactions for thsi block height. Same response format as `/blocks/latest/transactions`._

`GET /transactions/<txid>`

_Returns the transaction of this txid. Same response format as single item in the `transactions` list from above._

`GET /transactions/<txid>/scalar`

_Returns the scalar for this tx. only without the other transaction data._

```json
{
  "scalar": "0300260cd166b0b9375963fdeea829c638ad74e69ddba80a43bf3388619d2ee96d"
}
```

## Websocket subscriptions

`/ws/scalars`

_Subscribes to new scalars. Streamed messages are JSON from `/blocks/<height>/scalars`._

`/ws/transactions`

_Subscribes to new transactions. Streamed messages are JSON from `/blocks/<height>/transactions`._


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
