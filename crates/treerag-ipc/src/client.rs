//! IPC client for communicating with the TreeRAG daemon.

use crate::{IpcError, Request, Response};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

/// Default socket path
const DEFAULT_SOCKET_PATH: &str = "/tmp/treerag.sock";

/// Connection timeout
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

/// Request/response timeout
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// IPC client for communicating with the daemon
pub struct IpcClient {
    socket_path: PathBuf,
}

impl IpcClient {
    /// Create a client with default socket path
    pub fn new() -> Self {
        Self {
            socket_path: PathBuf::from(DEFAULT_SOCKET_PATH),
        }
    }

    /// Create a client with custom socket path
    pub fn with_socket_path<P: AsRef<Path>>(socket_path: P) -> Self {
        Self {
            socket_path: socket_path.as_ref().to_path_buf(),
        }
    }

    /// Connect to the daemon and return a connected client
    pub async fn connect() -> Result<ConnectedClient, IpcError> {
        Self::new().do_connect().await
    }

    /// Connect with custom socket path
    pub async fn connect_to<P: AsRef<Path>>(socket_path: P) -> Result<ConnectedClient, IpcError> {
        Self::with_socket_path(socket_path).do_connect().await
    }

    async fn do_connect(&self) -> Result<ConnectedClient, IpcError> {
        if !self.socket_path.exists() {
            return Err(IpcError::DaemonNotRunning);
        }

        let stream = tokio::time::timeout(CONNECT_TIMEOUT, UnixStream::connect(&self.socket_path))
            .await
            .map_err(|_| IpcError::ConnectionFailed("Connection timed out".to_string()))??;

        Ok(ConnectedClient { stream })
    }

    /// Send a fire-and-forget request (don't wait for response)
    pub async fn send_async(&self, request: &Request) -> Result<(), IpcError> {
        if !self.socket_path.exists() {
            return Err(IpcError::DaemonNotRunning);
        }

        let mut stream = UnixStream::connect(&self.socket_path).await?;

        let request_bytes = rmp_serde::to_vec(request)?;
        let len_bytes = (request_bytes.len() as u32).to_le_bytes();

        stream.write_all(&len_bytes).await?;
        stream.write_all(&request_bytes).await?;

        // Don't wait for response
        Ok(())
    }

    /// Check if daemon is running
    pub fn is_daemon_running(&self) -> bool {
        self.socket_path.exists()
    }
}

impl Default for IpcClient {
    fn default() -> Self {
        Self::new()
    }
}

/// A connected IPC client that can send requests and receive responses
pub struct ConnectedClient {
    stream: UnixStream,
}

impl ConnectedClient {
    /// Send a request and wait for response
    pub async fn send(&mut self, request: Request) -> Result<Response, IpcError> {
        tokio::time::timeout(REQUEST_TIMEOUT, self.do_send(request))
            .await
            .map_err(|_| IpcError::ConnectionFailed("Request timed out".to_string()))?
    }

    async fn do_send(&mut self, request: Request) -> Result<Response, IpcError> {
        // Serialize request
        let request_bytes = rmp_serde::to_vec(&request)?;
        let len_bytes = (request_bytes.len() as u32).to_le_bytes();

        // Send request
        self.stream.write_all(&len_bytes).await?;
        self.stream.write_all(&request_bytes).await?;
        self.stream.flush().await?;

        // Read response length
        let mut len_buf = [0u8; 4];
        self.stream.read_exact(&mut len_buf).await?;
        let len = u32::from_le_bytes(len_buf) as usize;

        // Read response body
        let mut response_buf = vec![0u8; len];
        self.stream.read_exact(&mut response_buf).await?;

        // Deserialize response
        let response: Response = rmp_serde::from_slice(&response_buf)?;

        Ok(response)
    }
}

/// Convenience functions for one-off requests
impl IpcClient {
    /// Send a request and wait for response (opens new connection)
    pub async fn request(&self, request: Request) -> Result<Response, IpcError> {
        let mut client = self.do_connect().await?;
        client.send(request).await
    }

    /// Check if a project is initialized
    pub async fn is_project_initialized(&self, cwd: &Path) -> Result<bool, IpcError> {
        let response = self
            .request(Request::CheckInit {
                cwd: cwd.to_path_buf(),
            })
            .await?;

        match response {
            Response::Ok {
                data: Some(crate::ResponseData::InitStatus { initialized }),
            } => Ok(initialized),
            Response::Error { message, .. } => Err(IpcError::ConnectionFailed(message)),
            _ => Ok(false),
        }
    }

    /// Get daemon status
    pub async fn get_status(&self) -> Result<crate::ResponseData, IpcError> {
        let response = self.request(Request::Status).await?;

        match response {
            Response::Ok { data: Some(data) } => Ok(data),
            Response::Error { message, .. } => Err(IpcError::ConnectionFailed(message)),
            _ => Err(IpcError::ConnectionFailed(
                "Unexpected response".to_string(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{IpcServer, RequestHandler, ResponseData};
    use async_trait::async_trait;
    use std::sync::Arc;
    use tempfile::tempdir;

    struct TestHandler;

    #[async_trait]
    impl RequestHandler for TestHandler {
        async fn handle(&self, request: Request) -> Response {
            match request {
                Request::Ping => Response::ok_with(ResponseData::Pong { timestamp: 0 }),
                Request::Status => Response::ok_with(ResponseData::Status {
                    version: "test".to_string(),
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
    async fn test_client_connect_no_daemon() {
        let client = IpcClient::with_socket_path("/tmp/nonexistent_socket_12345.sock");
        let result = client.do_connect().await;
        assert!(matches!(result, Err(IpcError::DaemonNotRunning)));
    }

    #[tokio::test]
    async fn test_client_is_daemon_running() {
        let client = IpcClient::with_socket_path("/tmp/nonexistent_socket_12345.sock");
        assert!(!client.is_daemon_running());
    }

    #[tokio::test]
    async fn test_client_default() {
        let client = IpcClient::default();
        assert_eq!(client.socket_path, PathBuf::from(DEFAULT_SOCKET_PATH));
    }

    #[tokio::test]
    async fn test_client_connect_and_ping() {
        let temp_dir = tempdir().unwrap();
        let socket_path = temp_dir.path().join("test.sock");

        // Start server
        let handler = Arc::new(TestHandler);
        let server = IpcServer::new(&socket_path, handler).await.unwrap();

        tokio::spawn(async move {
            let _ = server.run().await;
        });

        // Wait for server to start
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Connect and send ping
        let client = IpcClient::with_socket_path(&socket_path);
        let response = client.request(Request::Ping).await.unwrap();

        assert!(matches!(
            response,
            Response::Ok {
                data: Some(ResponseData::Pong { .. })
            }
        ));
    }

    #[tokio::test]
    async fn test_client_get_status() {
        let temp_dir = tempdir().unwrap();
        let socket_path = temp_dir.path().join("test.sock");

        // Start server
        let handler = Arc::new(TestHandler);
        let server = IpcServer::new(&socket_path, handler).await.unwrap();

        tokio::spawn(async move {
            let _ = server.run().await;
        });

        tokio::time::sleep(Duration::from_millis(50)).await;

        let client = IpcClient::with_socket_path(&socket_path);
        let status = client.get_status().await.unwrap();

        assert!(matches!(status, ResponseData::Status { .. }));
    }

    #[tokio::test]
    async fn test_client_send_async() {
        let temp_dir = tempdir().unwrap();
        let socket_path = temp_dir.path().join("test.sock");

        // Start server
        let handler = Arc::new(TestHandler);
        let server = IpcServer::new(&socket_path, handler).await.unwrap();

        tokio::spawn(async move {
            let _ = server.run().await;
        });

        tokio::time::sleep(Duration::from_millis(50)).await;

        let client = IpcClient::with_socket_path(&socket_path);
        let result = client.send_async(&Request::Ping).await;

        // Fire-and-forget should succeed
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_client_send_async_no_daemon() {
        let client = IpcClient::with_socket_path("/tmp/nonexistent_socket_12345.sock");
        let result = client.send_async(&Request::Ping).await;
        assert!(matches!(result, Err(IpcError::DaemonNotRunning)));
    }
}
