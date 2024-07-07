use candid::{CandidType, Nat};
use ic_cdk::api::management_canister::http_request::{
    http_request, CanisterHttpRequestArgument, HttpHeader, HttpResponse, TransformArgs,
    TransformContext,
};
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(CandidType, Default, Clone, Deserialize, Serialize)]
pub struct Agent {
    pub name: String, // used as a prefix for idempotency_key and message in sign_proxy_token to separate different business processes.
    pub endpoint: String,
    pub max_cycles: u64,
    pub proxy_token: Option<String>,
}

impl Agent {
    fn build_request(&self, req: &mut CanisterHttpRequestArgument) -> Result<(), String> {
        if !req.headers.iter().any(|h| h.name == "idempotency-key") {
            Err("idempotency-key header is missing".to_string())?;
        }

        if req.url.starts_with("URL_") {
            req.url = format!("{}/{}", self.endpoint, req.url);
        } else {
            let url = Url::parse(&req.url)
                .map_err(|err| format!("parse url {} error: {}", req.url, err))?;
            let host = url
                .host_str()
                .ok_or_else(|| format!("url host is empty: {}", req.url))?;
            req.headers.push(HttpHeader {
                name: "x-forwarded-host".to_string(),
                value: host.to_string(),
            });
            req.url.clone_from(&self.endpoint);
        }

        if !req.headers.iter().any(|h| h.name == "response-headers") {
            req.headers.push(HttpHeader {
                name: "response-headers".to_string(),
                value: "date".to_string(),
            });
        }

        if let Some(proxy_token) = &self.proxy_token {
            req.headers.push(HttpHeader {
                name: "proxy-authorization".to_string(),
                value: format!("Bearer {}", proxy_token),
            });
        }

        req.transform = Some(TransformContext::from_name(
            "inner_transform_response".to_string(),
            vec![],
        ));

        Ok(())
    }

    pub async fn call(&self, mut req: CanisterHttpRequestArgument) -> HttpResponse {
        if let Err(err) = self.build_request(&mut req) {
            return HttpResponse {
                status: Nat::from(400u64),
                body: err.into_bytes(),
                headers: vec![],
            };
        }

        match http_request(req, self.max_cycles as u128).await {
            Ok((res,)) => res,
            Err((code, message)) => HttpResponse {
                status: Nat::from(503u64),
                body: format!("http_request resulted into error. code: {code:?}, error: {message}")
                    .into_bytes(),
                headers: vec![],
            },
        }
    }
}

#[ic_cdk::query(hidden = true)]
fn inner_transform_response(args: TransformArgs) -> HttpResponse {
    HttpResponse {
        status: args.response.status,
        body: args.response.body,
        // Remove headers (which may contain a timestamp) for consensus
        headers: vec![],
    }
}
