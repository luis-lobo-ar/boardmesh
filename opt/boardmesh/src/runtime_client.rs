use crate::runtime::{JsonRpcRequest, JsonRpcResponse};
use anyhow::Context;
use serde::Serialize;
use serde::de::DeserializeOwned;
use tungstenite::{Message as WsMessage, connect as ws_connect};

pub fn runtime_request<TParams: Serialize, TResult: DeserializeOwned>(
    runtime_url: &str,
    method: &str,
    params: TParams,
) -> anyhow::Result<TResult> {
    let (mut socket, _) = ws_connect(runtime_url)
        .map_err(|error| anyhow::anyhow!("failed to connect to runtime websocket: {error}"))?;
    let request = JsonRpcRequest::new(1, method, params);
    socket
        .send(WsMessage::Text(
            serde_json::to_string(&request).context("failed to encode runtime request")?,
        ))
        .map_err(|error| anyhow::anyhow!("failed to send runtime request: {error}"))?;

    loop {
        let message = socket
            .read()
            .map_err(|error| anyhow::anyhow!("failed to read runtime response: {error}"))?;
        match message {
            WsMessage::Text(text) => {
                let response = serde_json::from_str::<JsonRpcResponse>(text.as_ref())
                    .map_err(|error| anyhow::anyhow!("invalid runtime response: {error}"))?;
                if let Some(error) = response.error {
                    anyhow::bail!("runtime error {}: {}", error.code, error.message);
                }
                let result = response
                    .result
                    .ok_or_else(|| anyhow::anyhow!("runtime response missing result"))?;
                return serde_json::from_value(result)
                    .map_err(|error| anyhow::anyhow!("invalid runtime payload: {error}"));
            }
            WsMessage::Close(_) => anyhow::bail!("runtime websocket closed before responding"),
            _ => {}
        }
    }
}
