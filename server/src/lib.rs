use bitcoin::hashes::Hash;
use bitcoincore_rpc::bitcoin::{
    OutPoint, ScriptBuf, Transaction, TxIn, TxOut, Witness, WitnessVersion,
};
use secp256k1::{Parity, PublicKey, XOnlyPublicKey};

pub mod server;
pub mod store;
pub mod sync;
pub mod config;

#[cfg(test)]
pub mod tests;

mod error;

pub use self::error::{Error, Result};
pub use self::config::Config;

#[derive(Debug, Clone)]
pub struct SPBlock {
    pub height: u64,
    pub hash: String,
    pub txs: Vec<SPTransaction>,
}

#[derive(Debug, Clone)]
pub struct SPTransaction {
    // Edited bitcoin transaction. Contains only taproot outputs.
    pub tx: Transaction,
    // Hex encoded public tweak.
    pub scalar: String,
}

fn serialize_outpoint(outpoint: &OutPoint) -> Vec<u8> {
    let mut buf = Vec::new();

    // Write the txid (Hash) as bytes
    buf.extend_from_slice(&outpoint.txid[..]);

    // Convert the index (vout) to little-endian bytes manually
    let index_bytes = outpoint.vout.to_le_bytes();
    buf.extend_from_slice(&index_bytes);

    buf
}

// hash = sha256(sha256(tag) || sha256(tag) || msg)
fn hash_tag_inputs(msg: &[u8]) -> [u8; 32] {
    let tag = b"BIP0352/Inputs";
    let tag_hash = bitcoin::hashes::sha256::Hash::hash(tag);
    let tag_tag_msg = [tag_hash.as_ref(), tag_hash.as_ref(), msg].concat();
    let hash = bitcoin::hashes::sha256::Hash::hash(&tag_tag_msg).to_byte_array();
    hash
}
fn calculate_input_hash(outpoint: OutPoint, public_key_sum: PublicKey) -> [u8; 32] {
    let outpoint_ser = serialize_outpoint(&outpoint);
    let public_key_ser = public_key_sum.serialize();
    let msg = [outpoint_ser.as_slice(), &public_key_ser].concat();
    let input_hash = hash_tag_inputs(msg.as_slice());
    input_hash
}

pub fn has_taproot_outputs(tx: &Transaction) -> bool {
    tx.output.iter().any(|txout| txout.script_pubkey.is_p2tr())
}

pub fn has_output_witness_version_greater_v1(outputs: &Vec<TxOut>) -> bool {
    outputs
        .iter()
        .any(|txout| output_witness_version_greater_v1(txout))
}

pub fn has_input_for_shared_secret(inputs: &Vec<TxIn>, prevouts: &Vec<TxOut>) -> bool {
    inputs
        .iter()
        .zip(prevouts)
        .any(|(txin, prevout)| is_input_for_shared_secret(txin, prevout))
}

pub fn output_witness_version_greater_v1(txout: &TxOut) -> bool {
    match txout.script_pubkey.witness_version() {
        Some(version) => version > WitnessVersion::V1,
        None => false,
    }
}

pub fn is_input_for_shared_secret(txin: &TxIn, prevout: &TxOut) -> bool {
    let s = &prevout.script_pubkey;
    let is_p2wpkh_witness = txin.witness.len() == 2;
    s.is_p2tr() || s.is_p2wpkh() || s.is_p2pkh() || (s.is_p2sh() && is_p2wpkh_witness)
}

pub fn try_get_input_public_key(input: &TxIn, prevout: &TxOut) -> Result<PublicKey> {
    let prevout_script_pubkey = &prevout.script_pubkey;

    // P2TR
    if prevout_script_pubkey.is_p2tr() {
        return get_p2tr_input_public_key(input, prevout_script_pubkey);
    }

    // P2WPKH
    if prevout_script_pubkey.is_p2wpkh() {
        return get_p2wpkh_input_public_key(&input.witness);
    }

    // P2SH
    if prevout_script_pubkey.is_p2sh() {
        return get_p2sh_p2wpkh_input_public_key(input, &input.witness);
    }

    // P2PKH
    if prevout_script_pubkey.is_p2pkh() {
        return get_p2pkh_input_public_key(input, prevout_script_pubkey);
    }

    Err(Error::InvalidInput)
}

fn get_p2pkh_input_public_key(input: &TxIn, prevout_spk: &ScriptBuf) -> Result<PublicKey> {
    let script_sig_bytes = input.script_sig.as_bytes();
    // DUP HASH160 <public-key-hash> EQUALVERIFY CHECKSIG
    // <public-key-hash> is data so there is actually one length byte before it. So we skip first 3
    // bytes and take 20 byte slice.
    let public_key_hash = &prevout_spk.as_bytes()[3..23];
    for i in 0..script_sig_bytes.len() {
        if i + 33 > script_sig_bytes.len() {
            break;
        }
        let frame = &script_sig_bytes[i..i + 33];
        let frame_hash = bitcoin::hashes::hash160::Hash::hash(frame).to_byte_array();
        if public_key_hash == frame_hash {
            // Frame bytes are public key bytes.
            let public_key =
                PublicKey::from_slice(frame).expect("Frame bytes are valid public key bytes.");
            return Ok(public_key);
        }
    }
    Err(Error::InvalidInput)
}

fn get_p2wpkh_input_public_key(witness: &Witness) -> Result<PublicKey> {
    let key_bytes = witness.nth(1);
    if let Some(bytes) = key_bytes {
        if bytes.len() == 33 {
            let public_key = PublicKey::from_slice(bytes)
                .expect("Compressed Public Key bytes from witness are valid.");
            return Ok(public_key);
        }
    }
    Err(Error::InvalidInput)
}

fn get_p2sh_p2wpkh_input_public_key(input: &TxIn, witness: &Witness) -> Result<PublicKey> {
    let witness_program_bytes = &input.script_sig.as_bytes()[1..];
    let witness_program_script = ScriptBuf::from_bytes(witness_program_bytes.to_vec());

    if witness_program_script.is_p2wpkh() {
        return get_p2wpkh_input_public_key(witness);
    }
    Err(Error::InvalidInput)
}

// Get the public key of a taproot input. Perform validation as defined in BIP-341 Specification,
// Script validation rules (incomplete, we don't check the signature). Input has to use Key spend
// path otherwise error is returned.
fn get_p2tr_input_public_key(input: &TxIn, prevout_spk: &ScriptBuf) -> Result<PublicKey> {
    // Fail if the witness stack has 0 elements.
    if input.witness.len() == 0 {
        return Err(Error::InvalidInput);
    }

    // If there are at least two witness elements, and the first byte of the last element is 0x50,
    // this last element is called annex and is removed from the witness stack. (...).
    if input.witness.len() >= 2 {
        if let Some(_annex) = input.witness.taproot_annex() {
            // If there is exactly one element left in the witness stack, key path spending is used.
            if input.witness.len() != 2 {
                return Err(Error::InvalidInput);
            }
            // TODO: Check valid signature for "q" (See BIP-341).
        } else {
            // More than one elements on witness stack but no annex means script path spend is used
            // which we regard as invalid in this context.
            return Err(Error::InvalidInput);
        }
    }

    // Key path spend is used.
    // scriptPubKey is OP_1 0x20 <32 byte x-only public key> so we take the slice without the OP_1
    // and 0x20.
    let pubkey_bytes = &prevout_spk.as_bytes()[2..];
    let x_only_public_key = XOnlyPublicKey::from_slice(pubkey_bytes).unwrap();
    let public_key = PublicKey::from_x_only_public_key(x_only_public_key, Parity::Even);
    Ok(public_key)
}
