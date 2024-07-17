use candid::Principal;
use ic_cdk::api::management_canister::http_request::{
    CanisterHttpRequestArgument, HttpHeader, HttpMethod, HttpResponse,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{to_vec, Value};

pub static APP_AGENT: &str = concat!(
    "Mozilla/5.0 eth-canister ",
    env!("CARGO_PKG_NAME"),
    "/",
    env!("CARGO_PKG_VERSION"),
);

pub struct EthereumRPC {
    pub provider: String, // provider url or `URL_` constant defined in the Idempotent Proxy
    pub proxy: Principal, // idempotent-proxy-canister id
    pub api_token: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RPCRequest<'a> {
    jsonrpc: &'a str,
    method: &'a str,
    params: &'a [Value],
    id: u64,
}

#[derive(Debug, Deserialize)]
pub struct RPCResponse<T> {
    result: Option<T>,
    error: Option<Value>,
}

impl EthereumRPC {
    pub async fn eth_chain_id(&self, idempotency_key: String) -> Result<String, String> {
        self.call("proxy_http_request", idempotency_key, "eth_chainId", &[])
            .await
    }

    pub async fn get_best_block(&self, idempotency_key: String) -> Result<Value, String> {
        self.call(
            "parallel_call_all_ok",
            idempotency_key,
            "eth_getBlockByNumber",
            &["latest".into(), false.into()],
        )
        .await
    }

    pub async fn send_raw_transaction(
        &self,
        idempotency_key: String,
        raw_tx: String,
    ) -> Result<String, String> {
        self.call(
            "parallel_call_any_ok",
            idempotency_key,
            "eth_sendTransaction",
            &[raw_tx.into()],
        )
        .await
    }

    // you can add more methods here

    pub async fn call<T: DeserializeOwned>(
        &self,
        proxy_method: &str, // "proxy_http_request" | "parallel_call_any_ok" | "parallel_call_all_ok"
        idempotency_key: String,
        method: &str,
        params: &[Value],
    ) -> Result<T, String> {
        let input = RPCRequest {
            jsonrpc: "2.0",
            method,
            params,
            id: 1,
        };
        let input = to_vec(&input).map_err(|err| err.to_string())?;
        let data = self.proxy(proxy_method, idempotency_key, input).await?;

        let output: RPCResponse<T> =
            serde_json::from_slice(&data).map_err(|err| err.to_string())?;

        if let Some(error) = output.error {
            return Err(serde_json::to_string(&error).map_err(|err| err.to_string())?);
        }

        match output.result {
            Some(result) => Ok(result),
            None => serde_json::from_value(Value::Null).map_err(|err| err.to_string()),
        }
    }

    async fn proxy(
        &self,
        proxy_method: &str,
        idempotency_key: String,
        body: Vec<u8>,
    ) -> Result<Vec<u8>, String> {
        let mut request_headers = vec![
            HttpHeader {
                name: "content-type".to_string(),
                value: "application/json".to_string(),
            },
            HttpHeader {
                name: "user-agent".to_string(),
                value: APP_AGENT.to_string(),
            },
            HttpHeader {
                name: "idempotency-key".to_string(),
                value: idempotency_key.clone(),
            },
        ];

        if let Some(api_token) = &self.api_token {
            request_headers.push(HttpHeader {
                name: "authorization".to_string(),
                value: api_token.clone(),
            });
        }

        let request = CanisterHttpRequestArgument {
            url: self.provider.clone(),
            max_response_bytes: None, //optional for request
            method: HttpMethod::POST,
            headers: request_headers,
            body: Some(body),
            transform: None,
        };

        let (res,): (HttpResponse,) = ic_cdk::api::call::call_with_payment128(
            self.proxy,
            proxy_method,
            (request,),
            1_000_000_000, // max cycles, unspent cycles will be refunded
        )
        .await
        .map_err(|(code, msg)| {
            format!(
                "failed to call {} on {:?}, code: {}, message: {}",
                proxy_method, &self.proxy, code as u32, msg
            )
        })?;

        if res.status >= 200u64 && res.status < 300u64 {
            Ok(res.body)
        } else {
            Err(format!(
                "failed to request provider: {}, idempotency-key: {}, status: {}, body: {}",
                self.provider,
                idempotency_key,
                res.status,
                String::from_utf8(res.body).unwrap_or_default(),
            ))
        }
    }
}
