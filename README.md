# BOSS POC: Silent Payment indexer

- Store and serve tweak data.
- Store and serve taproot output transactions.
- Given a mnemonic code, detect outputs to silent payment address.

# Plan

## JSON-RPC vs. gRPC

Performance should not be an issue because the data that has to be serialized will have minimal
structure (e.g a list with lots of items in contrast to a complex object) so gRPC for performance
would be overkill. Because of this I wanted to choose JSON-RPC because it is easy to understand,
however for DX I think I'll use gRPC because of the nice [tonic](https://crates.io/crates/tonic)
crate I can use for that so I can focus on the silent payment stuff instead of implementing a
JSON-RPC server too (would need to implement HTTP/WS transport and most of JSON-2.0 RPC protocol
myself, there are some crates for primitives and even full frameworks but they are not good/easy to
use..).


## Functionality

- Subscribe new tweaks
- Get tweaks (by block, by txids, or all)
- Get transactions (by block, by txids, by filter, or all)
- Get output, provided a shared secret, B_spend and a list of labels
  ask the server to find outputs.
- Subscribe outputs, given a mnemonic or key-pair subscribe to outputs to this wallet only
  instead of subscribing to partial tweaks, calculating the SS and then asking the server to
  find the output.

## Limiting scope

- Serve DB tweaks only: the server won't calculate tweak data for some tx. or even block on demand
  even though it could. Just to keep things simple. If queried block/tx is not in DB, error.
- Server subscirbes to new transactions from the point it was started only. It won't sync. from a
  certain block height.
- Server does not keep track of utxos, clients have to figure out whether the output for a tx. with
  a matching tweak is a utxo for their wallet or history.
- Don't deal with chain re-orgs.
- Don't process incomming txs (mempool)


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
