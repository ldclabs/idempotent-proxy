use candid::{CandidType, Nat};
use http::Uri;
use ic_cdk::api::management_canister::http_request::{
    http_request, CanisterHttpRequestArgument, HttpHeader, HttpResponse, TransformArgs,
    TransformContext,
};
use serde::{Deserialize, Serialize};

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
            let url: Uri = req
                .url
                .parse()
                .map_err(|err| format!("parse url {} error: {}", req.url, err))?;
            let host = url
                .host()
                .ok_or_else(|| format!("url host is empty: {}", req.url))?;
            req.headers.push(HttpHeader {
                name: "x-forwarded-host".to_string(),
                value: host.to_string(),
            });

            let path_query = url.path_and_query().map(|v| v.as_str()).unwrap_or("/");

            req.url = format!("{}{}", self.endpoint, path_query);
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

    pub async fn call(
        &self,
        mut req: CanisterHttpRequestArgument,
    ) -> Result<HttpResponse, HttpResponse> {
        if let Err(err) = self.build_request(&mut req) {
            return Ok(HttpResponse {
                status: Nat::from(400u64),
                body: err.into_bytes(),
                headers: vec![],
            });
        }

        match http_request(req, self.max_cycles as u128).await {
            Ok((res,)) if res.status <= 500u64 => Ok(res),
            Ok((res,)) => Err(res),
            Err((code, message)) => Err(HttpResponse {
                status: Nat::from(503u64),
                body: format!("http_request resulted into error. code: {code:?}, error: {message}")
                    .into_bytes(),
                headers: vec![],
            }),
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
