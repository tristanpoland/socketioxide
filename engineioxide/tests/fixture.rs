use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::Duration,
};

use bytes::Buf;
use engineioxide::{config::EngineIoConfig, handler::EngineIoHandler, service::EngineIoService};
use http::Request;
use http_body_util::{Either, Empty, Full};
use hyper::{client::legacy::Client, server::conn::http1};
use hyper_util::rt::TokioIo;
use serde::{Deserialize, Serialize};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

/// An OpenPacket is used to initiate a connection
#[derive(Debug, Serialize, Deserialize, PartialEq, PartialOrd)]
#[serde(rename_all = "camelCase")]
struct OpenPacket {
    sid: String,
    upgrades: Vec<String>,
    ping_interval: u64,
    ping_timeout: u64,
    max_payload: u64,
}

/// Params should be in the form of `key1=value1&key2=value2`
pub async fn send_req(
    port: u16,
    params: String,
    method: http::Method,
    body: Option<String>,
) -> String {
    let body = match body {
        Some(b) => Either::Left(Full::from(b)),
        None => Either::Right(Empty::new()),
    };

    let req = Request::builder()
        .method(method)
        .uri(format!(
            "http://127.0.0.1:{port}/engine.io/?EIO=4&{}",
            params
        ))
        .body(body)
        .unwrap();
    let mut res = Client::new().request(req).await.unwrap();
    let body = http_body_util::Collected::aggregate(res.body_mut());
    String::from_utf8(body.chunk().to_vec())
        .unwrap()
        .chars()
        .skip(1)
        .collect()
}

pub async fn create_polling_connection(port: u16) -> String {
    let body = send_req(port, format!("transport=polling"), http::Method::GET, None).await;
    let open_packet: OpenPacket = serde_json::from_str(&body).unwrap();
    open_packet.sid
}
pub async fn create_ws_connection(port: u16) -> WebSocketStream<MaybeTlsStream<TcpStream>> {
    tokio_tungstenite::connect_async(format!(
        "ws://127.0.0.1:{port}/engine.io/?EIO=4&transport=websocket"
    ))
    .await
    .unwrap()
    .0
}

pub fn create_server<H: EngineIoHandler>(handler: H, port: u16) {
    let config = EngineIoConfig::builder()
        .ping_interval(Duration::from_millis(300))
        .ping_timeout(Duration::from_millis(200))
        .max_payload(1e6 as u64)
        .build();

    let addr = &SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);

    let svc = EngineIoService::with_config(handler, config);

    tokio::spawn(async move {
        let listener = TcpListener::bind("127.0.0.1:3000").await.unwrap();

        // We start a loop to continuously accept incoming connections
        loop {
            let (stream, _) = listener.accept().await.unwrap();

            // Use an adapter to access something implementing `tokio::io` traits as if they implement
            // `hyper::rt` IO traits.
            let io = TokioIo::new(stream);
            let svc = svc.clone();

            // Spawn a tokio task to serve multiple connections concurrently
            tokio::task::spawn(async move {
                // Finally, we bind the incoming connection to our `hello` service
                if let Err(err) = http1::Builder::new()
                    .serve_connection(io, svc)
                    .with_upgrades()
                    .await
                {
                    println!("Error serving connection: {:?}", err);
                }
            });
        }
    });
}
