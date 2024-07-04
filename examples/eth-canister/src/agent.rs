use async_trait::async_trait;
use candid::CandidType;
use ic_cdk::api::management_canister::http_request::{
    http_request, CanisterHttpRequestArgument, HttpHeader, HttpMethod, HttpResponse, TransformArgs,
    TransformContext,
};
use serde::{Deserialize, Serialize};

use crate::jsonrpc::{JsonRPCAgent, APP_AGENT};

#[derive(CandidType, Default, Clone, Deserialize, Serialize)]
pub struct RPCAgent {
    pub name: String, // used as a prefix for idempotency_key and message in sign_proxy_token to separate different business processes.
    pub endpoint: String,
    pub max_cycles: u64,
    pub proxy_token: Option<String>,
    pub api_token: Option<String>,
}

#[async_trait]
impl JsonRPCAgent for &RPCAgent {
    async fn post(&self, idempotency_key: String, body: Vec<u8>) -> Result<Vec<u8>, String> {
        let mut request_headers = vec![
            HttpHeader {
                name: "content-type".to_string(),
                value: "application/json".to_string(),
            },
            HttpHeader {
                name: "user-agent".to_string(),
                value: APP_AGENT.to_string(),
            },
            // filter out all headers except "content-type", "content-length" and "date"
            // because this 3 headers will allways be returned from the server side
            HttpHeader {
                name: "response-headers".to_string(),
                value: "date".to_string(),
            },
            HttpHeader {
                name: "idempotency-key".to_string(),
                value: idempotency_key.clone(),
            },
        ];

        if let Some(proxy_token) = &self.proxy_token {
            request_headers.push(HttpHeader {
                name: "proxy-authorization".to_string(),
                value: format!("Bearer {}", proxy_token),
            });
        }

        if let Some(api_token) = &self.api_token {
            request_headers.push(HttpHeader {
                name: "authorization".to_string(),
                value: api_token.clone(),
            });
        }

        let request = CanisterHttpRequestArgument {
            url: self.endpoint.to_string(),
            max_response_bytes: None, //optional for request
            method: HttpMethod::POST,
            headers: request_headers,
            body: Some(body),
            transform: Some(TransformContext::from_name(
                "transform_jsonrpc".to_string(),
                vec![],
            )),
        };

        match http_request(request, self.max_cycles as u128).await {
            Ok((res,)) => {
                if res.status >= 200u64 && res.status < 300u64 {
                    Ok(res.body)
                } else {
                    Err(format!(
                        "failed to request url: {}, idempotency-key: {}, status: {}, body: {}",
                        self.endpoint,
                        idempotency_key,
                        res.status,
                        String::from_utf8(res.body).unwrap_or_default(),
                    ))
                }
            }
            Err((code, message)) => Err(format!(
                "the http_request resulted into error. code: {code:?}, error: {message}"
            )),
        }
    }
}

#[ic_cdk::query(hidden = true)]
fn transform_jsonrpc(args: TransformArgs) -> HttpResponse {
    HttpResponse {
        status: args.response.status,
        body: args.response.body,
        // Remove headers (which may contain a timestamp) for consensus
        headers: vec![],
    }
}
