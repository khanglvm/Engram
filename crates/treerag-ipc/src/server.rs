//! Unix socket IPC server for the TreeRAG daemon.
//!
//! Handles incoming connections and dispatches requests to handlers.

use crate::{IpcError, Request, Response};
use async_trait::async_trait;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};

/// Maximum request size (1MB)
const MAX_REQUEST_SIZE: usize = 1024 * 1024;

/// Request timeout for reading from socket
const REQUEST_TIMEOUT: Duration = Duration::from_millis(100);

/// Unix socket IPC server
pub struct IpcServer {
    listener: UnixListener,
    handler: Arc<dyn RequestHandler>,
}

impl IpcServer {
    /// Create a new IPC server bound to the given socket path
    pub async fn new<P: AsRef<Path>>(
        socket_path: P,
        handler: Arc<dyn RequestHandler>,
    ) -> Result<Self, IpcError> {
        let socket_path = socket_path.as_ref();

        // Remove stale socket file if it exists
        if socket_path.exists() {
            let _ = std::fs::remove_file(socket_path);
        }

        // Ensure parent directory exists
        if let Some(parent) = socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(socket_path)?;

        // Set socket permissions (user only - 0600)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o600))?;
        }

        tracing::info!("IPC server listening on {}", socket_path.display());

        Ok(Self { listener, handler })
    }

    /// Run the server, accepting connections until shutdown
    pub async fn run(&self) -> Result<(), IpcError> {
        loop {
            match self.listener.accept().await {
                Ok((stream, _addr)) => {
                    let handler = self.handler.clone();
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(stream, handler).await {
                            tracing::debug!("Connection error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    tracing::error!("Accept error: {}", e);
                }
            }
        }
    }

    /// Handle a single connection
    async fn handle_connection(
        mut stream: UnixStream,
        handler: Arc<dyn RequestHandler>,
    ) -> Result<(), IpcError> {
        // Read request with timeout to avoid blocking
        let request = tokio::time::timeout(REQUEST_TIMEOUT, Self::read_request(&mut stream))
            .await
            .map_err(IpcError::Timeout)?;

        let request = match request {
            Ok(req) => req,
            Err(e) => {
                // Send error response
                let response = Response::error(
                    crate::ErrorCode::InvalidRequest,
                    format!("Failed to parse request: {}", e),
                );
                Self::write_response(&mut stream, &response).await?;
                return Err(e);
            }
        };

        tracing::debug!("Received request: {:?}", request);

        // Handle request
        let response = handler.handle(request).await;

        // Send response
        Self::write_response(&mut stream, &response).await?;

        Ok(())
    }

    /// Read a request from the stream
    async fn read_request(stream: &mut UnixStream) -> Result<Request, IpcError> {
        // Read length prefix (4 bytes, little-endian)
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).await?;
        let len = u32::from_le_bytes(len_buf) as usize;

        if len > MAX_REQUEST_SIZE {
            return Err(IpcError::RequestTooLarge);
        }

        // Read request body
        let mut buf = vec![0u8; len];
        stream.read_exact(&mut buf).await?;

        // Try MessagePack first, fall back to JSON for easier debugging
        if let Ok(request) = rmp_serde::from_slice(&buf) {
            return Ok(request);
        }

        // Try JSON as fallback (useful for testing with nc/socat)
        if let Ok(request) = serde_json::from_slice(&buf) {
            return Ok(request);
        }

        Err(IpcError::Deserialize(
            rmp_serde::from_slice::<Request>(&buf).unwrap_err(),
        ))
    }

    /// Write a response to the stream
    async fn write_response(stream: &mut UnixStream, response: &Response) -> Result<(), IpcError> {
        let response_bytes = rmp_serde::to_vec(response)?;
        let len_bytes = (response_bytes.len() as u32).to_le_bytes();

        stream.write_all(&len_bytes).await?;
        stream.write_all(&response_bytes).await?;
        stream.flush().await?;

        Ok(())
    }
}

/// Trait for handling incoming requests
#[async_trait]
pub trait RequestHandler: Send + Sync {
    /// Handle a request and return a response
    async fn handle(&self, request: Request) -> Response;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ResponseData;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixStream;

    struct TestHandler;

    #[async_trait]
    impl RequestHandler for TestHandler {
        async fn handle(&self, request: Request) -> Response {
            match request {
                Request::Ping => Response::ok_with(ResponseData::Pong {
                    timestamp: chrono::Utc::now().timestamp(),
                }),
                Request::Status => Response::ok_with(ResponseData::Status {
                    version: "0.1.0".to_string(),
                    uptime_secs: 0,
                    projects_loaded: 0,
                    memory_usage_bytes: 0,
                    requests_total: 0,
                    cache_hit_rate: 0.0,
                    avg_latency_ms: 0,
                }),
                _ => Response::ack(),
            }
        }
    }

    #[tokio::test]
    async fn test_server_ping() {
        let socket_path = "/tmp/treerag_test.sock";
        let _ = std::fs::remove_file(socket_path);

        let handler = Arc::new(TestHandler);
        let server = IpcServer::new(socket_path, handler).await.unwrap();

        // Spawn server
        tokio::spawn(async move {
            let _ = server.run().await;
        });

        // Give server time to start
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Connect and send request
        let mut stream = UnixStream::connect(socket_path).await.unwrap();

        let request = Request::Ping;
        let request_bytes = rmp_serde::to_vec(&request).unwrap();
        let len_bytes = (request_bytes.len() as u32).to_le_bytes();

        stream.write_all(&len_bytes).await.unwrap();
        stream.write_all(&request_bytes).await.unwrap();

        // Read response
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).await.unwrap();
        let len = u32::from_le_bytes(len_buf) as usize;

        let mut response_buf = vec![0u8; len];
        stream.read_exact(&mut response_buf).await.unwrap();

        let response: Response = rmp_serde::from_slice(&response_buf).unwrap();

        if let Response::Ok {
            data: Some(ResponseData::Pong { .. }),
        } = response
        {
            // Success
        } else {
            panic!("Expected Pong response, got {:?}", response);
        }

        // Cleanup
        let _ = std::fs::remove_file(socket_path);
    }
}
