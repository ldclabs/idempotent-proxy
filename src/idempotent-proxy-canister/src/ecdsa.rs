use base64::{engine::general_purpose::URL_SAFE_NO_PAD as base64_url, Engine};
use ciborium::into_writer;
use ic_cdk::api::management_canister::ecdsa;
use serde_bytes::ByteBuf;
use sha3::{Digest, Sha3_256};

// use Idempotent Proxy's Token: Token(pub u64, pub String, pub ByteBuf);
// https://github.com/ldclabs/idempotent-proxy/blob/main/src/idempotent-proxy-types/src/auth.rs#L15
pub async fn sign_proxy_token(
    key_name: &str,
    expire_at: u64, // UNIX timestamp, in seconds
    message: &str,  // use RPCAgent.name as message
) -> Result<String, String> {
    let mut buf: Vec<u8> = Vec::new();
    into_writer(&(expire_at, message), &mut buf).expect("failed to encode Token in CBOR format");
    let digest = sha3_256(&buf);
    let sig = sign_with(key_name, vec![b"sign_proxy_token".to_vec()], digest)
        .await
        .map_err(err_string)?;
    buf.clear();
    into_writer(&(expire_at, message, ByteBuf::from(sig)), &mut buf).map_err(err_string)?;
    Ok(base64_url.encode(buf))
}

pub async fn get_proxy_token_public_key(key_name: &str) -> Result<String, String> {
    let pk = public_key_with(key_name, vec![b"sign_proxy_token".to_vec()]).await?;
    Ok(base64_url.encode(pk.public_key))
}

pub async fn sign_with(
    key_name: &str,
    derivation_path: Vec<Vec<u8>>,
    message_hash: [u8; 32],
) -> Result<Vec<u8>, String> {
    let args = ecdsa::SignWithEcdsaArgument {
        message_hash: message_hash.to_vec(),
        derivation_path,
        key_id: ecdsa::EcdsaKeyId {
            curve: ecdsa::EcdsaCurve::Secp256k1,
            name: key_name.to_string(),
        },
    };

    let (response,): (ecdsa::SignWithEcdsaResponse,) = ecdsa::sign_with_ecdsa(args)
        .await
        .map_err(|err| format!("sign_with_ecdsa failed {:?}", err))?;

    Ok(response.signature)
}

pub async fn public_key_with(
    key_name: &str,
    derivation_path: Vec<Vec<u8>>,
) -> Result<ecdsa::EcdsaPublicKeyResponse, String> {
    let args = ecdsa::EcdsaPublicKeyArgument {
        canister_id: None,
        derivation_path,
        key_id: ecdsa::EcdsaKeyId {
            curve: ecdsa::EcdsaCurve::Secp256k1,
            name: key_name.to_string(),
        },
    };

    let (response,): (ecdsa::EcdsaPublicKeyResponse,) = ecdsa::ecdsa_public_key(args)
        .await
        .map_err(|err| format!("ecdsa_public_key failed {:?}", err))?;

    Ok(response)
}

pub fn err_string(err: impl std::fmt::Display) -> String {
    err.to_string()
}

pub fn sha3_256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha3_256::new();
    hasher.update(data);
    hasher.finalize().into()
}
