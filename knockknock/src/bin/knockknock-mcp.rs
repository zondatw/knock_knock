//! `knockknock-mcp` — Model Context Protocol server exposing every
//! `zpinger` pinger as a typed tool over stdio.
//!
//! Each tool returns a structured JSON result containing per-iteration
//! timings plus a summary, so an AI agent can both understand "did
//! this endpoint respond" and reason about latency. Tools default to a
//! single ping (`count = 1`) — that's the common "is this thing
//! reachable right now" question. The CLI's default of 3 doesn't carry
//! over.

use std::time::Duration;

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Content, Implementation, ServerCapabilities, ServerInfo},
    schemars::{self, JsonSchema},
    tool, tool_handler, tool_router,
    transport::stdio,
    ErrorData as McpError, ServerHandler, ServiceExt,
};
use serde::{Deserialize, Serialize};
use zpinger::{
    DnsPinger, GrpcPinger, HttpMethod, HttpPinger, MqttPinger, MqttVersion, Pinger, RecordType,
    TcpPinger, UdpPinger, WebSocketPinger,
};

const DEFAULT_COUNT: u64 = 1;
const DEFAULT_TIMEOUT_MS: u64 = 5_000;

// -- argument types ---------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
struct TargetArgs {
    /// Target endpoint, format `host:port` (e.g. `example.com:80`).
    target: String,
    /// Number of ping iterations. Defaults to 1.
    #[serde(default)]
    count: Option<u64>,
    /// Per-ping timeout in milliseconds. Defaults to 5000.
    #[serde(default)]
    timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct HttpPingArgs {
    /// HTTP / HTTPS URL, e.g. `http://example.com/api` or
    /// `https://example.com:443/`.
    target: String,
    /// HTTP method. Defaults to `get`.
    #[serde(default)]
    method: Option<HttpMethodArg>,
    #[serde(default)]
    count: Option<u64>,
    #[serde(default)]
    timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
enum HttpMethodArg {
    Connect,
    Get,
    Post,
    Put,
    Delete,
    Patch,
}

impl From<HttpMethodArg> for HttpMethod {
    fn from(value: HttpMethodArg) -> Self {
        match value {
            HttpMethodArg::Connect => HttpMethod::Connect,
            HttpMethodArg::Get => HttpMethod::Get,
            HttpMethodArg::Post => HttpMethod::Post,
            HttpMethodArg::Put => HttpMethod::Put,
            HttpMethodArg::Delete => HttpMethod::Delete,
            HttpMethodArg::Patch => HttpMethod::Patch,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
struct DnsPingArgs {
    /// DNS server, e.g. `8.8.8.8` or `dns.example.com:53`.
    server: String,
    /// Domain name to look up, e.g. `example.com`.
    query: String,
    /// DNS record type. Defaults to `a`.
    #[serde(default)]
    record_type: Option<RecordTypeArg>,
    #[serde(default)]
    count: Option<u64>,
    #[serde(default)]
    timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
enum RecordTypeArg {
    A,
    Aaaa,
    Cname,
    Mx,
    Ns,
    Txt,
}

impl From<RecordTypeArg> for RecordType {
    fn from(value: RecordTypeArg) -> Self {
        match value {
            RecordTypeArg::A => RecordType::A,
            RecordTypeArg::Aaaa => RecordType::Aaaa,
            RecordTypeArg::Cname => RecordType::Cname,
            RecordTypeArg::Mx => RecordType::Mx,
            RecordTypeArg::Ns => RecordType::Ns,
            RecordTypeArg::Txt => RecordType::Txt,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
struct MqttPingArgs {
    /// MQTT broker URL, e.g. `mqtt://broker.example.com:1883`,
    /// `mqtts://broker.example.com:8883`, or schemeless host:port.
    broker: String,
    /// MQTT client identifier. Defaults to a random `knockknock-XXX`.
    #[serde(default)]
    client_id: Option<String>,
    /// Speak MQTT 5 instead of the default MQTT 3.1.1.
    #[serde(default)]
    v5: bool,
    #[serde(default)]
    count: Option<u64>,
    #[serde(default)]
    timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct GrpcPingArgs {
    /// gRPC endpoint, e.g. `grpc://localhost:50051`,
    /// `grpcs://api.example.com:443`, or `http(s)://host:port`.
    endpoint: String,
    /// Service name passed in `HealthCheckRequest.service`.
    /// Defaults to empty (overall server health).
    #[serde(default)]
    service: Option<String>,
    #[serde(default)]
    count: Option<u64>,
    #[serde(default)]
    timeout_ms: Option<u64>,
}

// -- result type ------------------------------------------------------

#[derive(Debug, Serialize)]
struct Iteration {
    elapsed_ms: f64,
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct Summary {
    count: u64,
    recv: u64,
    lose: u64,
    lose_pct: u64,
    total_ms: f64,
}

#[derive(Debug, Serialize)]
struct PingReport {
    iterations: Vec<Iteration>,
    summary: Summary,
}

async fn run_pings(pinger: &dyn Pinger, count: u64) -> PingReport {
    let mut iterations = Vec::with_capacity(count as usize);
    let mut total = Duration::ZERO;
    let mut recv = 0u64;
    for _ in 0..count {
        match zpinger::timed(pinger).await {
            Ok(elapsed) => {
                total += elapsed;
                recv += 1;
                iterations.push(Iteration {
                    elapsed_ms: elapsed.as_secs_f64() * 1000.0,
                    success: true,
                    error: None,
                });
            }
            Err(e) => iterations.push(Iteration {
                elapsed_ms: 0.0,
                success: false,
                error: Some(e.to_string()),
            }),
        }
    }
    let lose = count - recv;
    PingReport {
        iterations,
        summary: Summary {
            count,
            recv,
            lose,
            lose_pct: if count == 0 { 0 } else { lose * 100 / count },
            total_ms: total.as_secs_f64() * 1000.0,
        },
    }
}

fn report_to_result(report: &PingReport) -> Result<CallToolResult, McpError> {
    let payload = serde_json::to_string_pretty(report)
        .map_err(|e| McpError::internal_error(format!("serialize ping report: {e}"), None))?;
    Ok(CallToolResult::success(vec![Content::text(payload)]))
}

fn count_or_default(c: Option<u64>) -> u64 {
    c.unwrap_or(DEFAULT_COUNT).max(1)
}

fn timeout_or_default(t: Option<u64>) -> Duration {
    Duration::from_millis(t.unwrap_or(DEFAULT_TIMEOUT_MS))
}

// -- server -----------------------------------------------------------

#[derive(Clone)]
pub struct KnockknockServer {
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl KnockknockServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "TCP ping — TCP connect + 1-byte probe + read. Measures full round trip from connect to first byte back."
    )]
    async fn tcp_ping(
        &self,
        Parameters(args): Parameters<TargetArgs>,
    ) -> Result<CallToolResult, McpError> {
        let count = count_or_default(args.count);
        let p = TcpPinger::new(args.target).with_timeout(timeout_or_default(args.timeout_ms));
        let report = run_pings(&p, count).await;
        report_to_result(&report)
    }

    #[tool(
        description = "UDP ping — sends one datagram from an ephemeral local socket, waits for a datagram in reply."
    )]
    async fn udp_ping(
        &self,
        Parameters(args): Parameters<TargetArgs>,
    ) -> Result<CallToolResult, McpError> {
        let count = count_or_default(args.count);
        let p = UdpPinger::new(args.target).with_timeout(timeout_or_default(args.timeout_ms));
        let report = run_pings(&p, count).await;
        report_to_result(&report)
    }

    #[tool(
        description = "HTTP / HTTPS ping — full HTTP/1.1 request + response. https:// uses rustls + webpki-roots. Method defaults to GET."
    )]
    async fn http_ping(
        &self,
        Parameters(args): Parameters<HttpPingArgs>,
    ) -> Result<CallToolResult, McpError> {
        let count = count_or_default(args.count);
        let method: HttpMethod = args.method.map(Into::into).unwrap_or(HttpMethod::Get);
        let p =
            HttpPinger::new(method, args.target).with_timeout(timeout_or_default(args.timeout_ms));
        let report = run_pings(&p, count).await;
        report_to_result(&report)
    }

    #[tool(
        description = "WebSocket ping — RFC 6455 upgrade handshake plus a control PING/PONG round trip. Accepts ws:// and wss://."
    )]
    async fn ws_ping(
        &self,
        Parameters(args): Parameters<TargetArgs>,
    ) -> Result<CallToolResult, McpError> {
        let count = count_or_default(args.count);
        let p = WebSocketPinger::new(args.target).with_timeout(timeout_or_default(args.timeout_ms));
        let report = run_pings(&p, count).await;
        report_to_result(&report)
    }

    #[tool(
        description = "DNS ping — sends one UDP query (RFC 1035) and validates the response (ID match, QR set, RCODE 0, question echoed)."
    )]
    async fn dns_ping(
        &self,
        Parameters(args): Parameters<DnsPingArgs>,
    ) -> Result<CallToolResult, McpError> {
        let count = count_or_default(args.count);
        let record_type = args.record_type.map(Into::into).unwrap_or(RecordType::A);
        let p = DnsPinger::new(args.server, args.query)
            .with_record_type(record_type)
            .with_timeout(timeout_or_default(args.timeout_ms));
        let report = run_pings(&p, count).await;
        report_to_result(&report)
    }

    #[tool(
        description = "MQTT ping — runs CONNECT/CONNACK + PINGREQ/PINGRESP + DISCONNECT. Pass v5=true for MQTT 5; default is 3.1.1. Accepts mqtt:// and mqtts://."
    )]
    async fn mqtt_ping(
        &self,
        Parameters(args): Parameters<MqttPingArgs>,
    ) -> Result<CallToolResult, McpError> {
        let count = count_or_default(args.count);
        let mut p = MqttPinger::new(args.broker).with_timeout(timeout_or_default(args.timeout_ms));
        if let Some(cid) = args.client_id {
            p = p.with_client_id(cid);
        }
        if args.v5 {
            p = p.with_version(MqttVersion::V5);
        }
        let report = run_pings(&p, count).await;
        report_to_result(&report)
    }

    #[tool(
        description = "gRPC ping — calls grpc.health.v1.Health/Check. Accepts grpc:// / http:// (plaintext H2C) and grpcs:// / https:// (TLS)."
    )]
    async fn grpc_ping(
        &self,
        Parameters(args): Parameters<GrpcPingArgs>,
    ) -> Result<CallToolResult, McpError> {
        let count = count_or_default(args.count);
        let mut p =
            GrpcPinger::new(args.endpoint).with_timeout(timeout_or_default(args.timeout_ms));
        if let Some(service) = args.service {
            p = p.with_service(service);
        }
        let report = run_pings(&p, count).await;
        report_to_result(&report)
    }
}

impl Default for KnockknockServer {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_handler]
impl ServerHandler for KnockknockServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            server_info: Implementation {
                name: "knockknock-mcp".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                ..Default::default()
            },
            instructions: Some(
                "knock_knock latency probe — every supported protocol exposed as a tool. \
                 Each tool returns per-iteration timings plus a summary. Default count is 1; \
                 increase via the count argument when you want statistical RTT info. \
                 Default timeout is 5000 ms; override via timeout_ms when probing slow endpoints."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

// -- entry point ------------------------------------------------------

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server = KnockknockServer::new();
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
