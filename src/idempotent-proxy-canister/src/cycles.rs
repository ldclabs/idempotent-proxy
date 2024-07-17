use ic_cdk::api::management_canister::http_request::{CanisterHttpRequestArgument, HttpResponse};

#[derive(Clone)]
pub struct Calculator {
    pub subnet_size: u64,
    pub service_fee: u64,
}

// https://github.com/internet-computer-protocol/evm-rpc-canister/blob/main/src/accounting.rs#L34
impl Calculator {
    // HTTP outcall cost calculation
    // See https://internetcomputer.org/docs/current/developer-docs/gas-cost#special-features
    pub const INGRESS_MESSAGE_RECEIVED_COST: u64 = 1_200_000;
    pub const INGRESS_MESSAGE_BYTE_RECEIVED_COST: u64 = 2_000;
    pub const HTTP_OUTCALL_REQUEST_BASE_COST: u64 = 3_000_000;
    pub const HTTP_OUTCALL_REQUEST_PER_NODE_COST: u64 = 60_000;
    pub const HTTP_OUTCALL_REQUEST_COST_PER_BYTE: u64 = 400;
    pub const HTTP_OUTCALL_RESPONSE_COST_PER_BYTE: u64 = 800;
    // Additional headers added to the request
    pub const HTTP_OUTCALL_REQUEST_OVERHEAD_BYTES: u64 = 150;
    // Additional cost of operating the canister per subnet node
    pub const CANISTER_OVERHEAD: u64 = 1_000_000;

    pub fn count_request_bytes(&self, req: &CanisterHttpRequestArgument) -> usize {
        if self.subnet_size == 0 {
            return 0;
        }
        let mut total = req.url.len() + req.body.as_ref().map(|v| v.len()).unwrap_or_default();
        for header in &req.headers {
            total += header.name.len() + header.value.len() + 3;
        }
        total
    }

    pub fn count_response_bytes(&self, res: &HttpResponse) -> usize {
        if self.subnet_size == 0 {
            return 0;
        }
        let mut total = 4 + res.body.as_slice().len();
        for header in &res.headers {
            total += header.name.len() + header.value.len() + 3;
        }
        total
    }

    pub fn ingress_cost(&self, ingress_bytes: usize) -> u128 {
        if self.subnet_size == 0 {
            return 0;
        }
        let cost_per_node = Self::INGRESS_MESSAGE_RECEIVED_COST
            + Self::INGRESS_MESSAGE_BYTE_RECEIVED_COST * ingress_bytes as u64;
        cost_per_node as u128 * self.subnet_size as u128
    }

    pub fn http_outcall_request_cost(&self, request_bytes: usize, duplicates: usize) -> u128 {
        if self.subnet_size == 0 {
            return 0;
        }
        let cost_per_node = Self::HTTP_OUTCALL_REQUEST_BASE_COST
            + Self::HTTP_OUTCALL_REQUEST_PER_NODE_COST * self.subnet_size
            + Self::HTTP_OUTCALL_REQUEST_COST_PER_BYTE
                * (Self::HTTP_OUTCALL_REQUEST_OVERHEAD_BYTES + request_bytes as u64)
            + Self::CANISTER_OVERHEAD;
        self.service_fee as u128
            + cost_per_node as u128 * (self.subnet_size * duplicates as u64) as u128
    }

    pub fn http_outcall_response_cost(&self, response_bytes: usize, duplicates: usize) -> u128 {
        if self.subnet_size == 0 {
            return 0;
        }
        let cost_per_node = Self::HTTP_OUTCALL_RESPONSE_COST_PER_BYTE * response_bytes as u64;
        cost_per_node as u128 * (self.subnet_size * duplicates as u64) as u128
    }
}
