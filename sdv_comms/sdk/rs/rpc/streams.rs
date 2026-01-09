// Copyright (C) 2026 The Android Open Source Project
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::pin::Pin;
use std::task::{Context, Poll};

use sdv_status_rs::SdvResult;
use sdv_status_rs::SdvStatus;
use sdv_status_rs::SdvStatusCode;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tonic::transport::server::Connected;

use openssl::x509::X509;

fn clone_io_error(err: &std::io::Error) -> std::io::Error {
    std::io::Error::new(err.kind(), err.to_string())
}

// TODO(b/382649975): remove this wrapper, once Tonic is patched or updated.
// The reason for this new type wrapper to exist is that server RPC stops to operate
// if TLS handshake fails. The reason for this is that in our case Tonic doesn't see
// SSL wrapper over the insecure stream and failure of establishing TLS connection
// is understood by Tonic as failure of listening TCP socket. If listening socket
// doesn't work then the server can't accept new connection and according to Tonic
// interpretation it doesn't make sense for server to operate.
// Because of that we have to defer revealing of the failed TLS handshake to
// a later data transfer stage.
// In addition this is a wrapper that implements Tonic server::Connected trait.
pub struct ServerSslStream<S>(pub Result<tokio_openssl::SslStream<S>, std::io::Error>);

impl<S> AsyncRead for ServerSslStream<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match &mut self.0 {
            Ok(stream) => Pin::new(stream).poll_read(cx, buf),
            Err(e) => Poll::Ready(Err(clone_io_error(e))),
        }
    }
}

impl<S> AsyncWrite for ServerSslStream<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        match &mut self.0 {
            Ok(stream) => Pin::new(stream).poll_write(cx, buf),
            Err(e) => Poll::Ready(Err(clone_io_error(e))),
        }
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        match &mut self.0 {
            Ok(stream) => Pin::new(stream).poll_flush(cx),
            Err(_) => Poll::Ready(Ok(())),
        }
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        match &mut self.0 {
            Ok(stream) => Pin::new(stream).poll_shutdown(cx),
            Err(_) => Poll::Ready(Ok(())),
        }
    }
}

#[derive(Clone)]
pub struct SslStreamConnectInfo {
    pub peer_cert: tonic::Result<X509>,
}

impl<T: Connected> Connected for ServerSslStream<T> {
    type ConnectInfo = SslStreamConnectInfo;

    fn connect_info(&self) -> Self::ConnectInfo {
        let ssl_stream = match &self.0 {
            Ok(ssl_stream) => ssl_stream,
            Err(e) => {
                return SslStreamConnectInfo {
                    peer_cert: Err(tonic::Status::unauthenticated(e.to_string())),
                };
            }
        };

        // TODO: cache this value for performance reasons.
        let peer_cert =
            extract_cert(ssl_stream).map_err(|e| tonic::Status::unauthenticated(e.to_string()));

        SslStreamConnectInfo { peer_cert }
    }
}

pub(crate) fn extract_cert<T>(ssl_stream: &tokio_openssl::SslStream<T>) -> SdvResult<X509> {
    let peer_cert = match ssl_stream.ssl().peer_certificate() {
        Some(peer_cert) => peer_cert,
        None => {
            return Err(SdvStatus::with_message(
                SdvStatusCode::Unauthenticated,
                "Certificate of server not found",
            ));
        }
    };

    Ok(peer_cert)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::task::noop_waker_ref;
    use tokio::io::duplex;

    #[test]
    fn test_extract_cert_no_peer_certificate() {
        let (client, _server) = duplex(1024);
        let ssl = openssl::ssl::Ssl::new(
            &openssl::ssl::SslContext::builder(openssl::ssl::SslMethod::tls_client())
                .unwrap()
                .build(),
        )
        .unwrap();
        let ssl_stream = tokio_openssl::SslStream::new(ssl, client).unwrap();
        let result = extract_cert(&ssl_stream);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status_code(), SdvStatusCode::Unauthenticated);
        assert!(err.to_string().contains("Certificate of server not found"));
    }

    #[test]
    fn test_server_ssl_stream_poll_err_stream() {
        let io_error = std::io::Error::other("test error");
        // Create a ServerSslStream with an Err variant
        let mut stream_wrapper: ServerSslStream<tokio::io::DuplexStream> =
            ServerSslStream(Err(io_error));

        let mut cx = Context::from_waker(noop_waker_ref());

        let mut buf = [0u8; 1];
        let mut read_buf = ReadBuf::new(&mut buf);
        let poll_result = Pin::new(&mut stream_wrapper).poll_read(&mut cx, &mut read_buf);
        match poll_result {
            Poll::Ready(Err(e)) => {
                assert_eq!(e.kind(), std::io::ErrorKind::Other);
                assert!(e.to_string().contains("test error"));
            }
            _ => panic!("poll_read for error stream should return Ready(Err)"),
        }

        let poll_result = Pin::new(&mut stream_wrapper).poll_write(&mut cx, &[0u8, 1u8]);
        match poll_result {
            Poll::Ready(Err(e)) => {
                assert_eq!(e.kind(), std::io::ErrorKind::Other);
                assert!(e.to_string().contains("test error"));
            }
            _ => panic!("poll_write for error stream should return Ready(Err)"),
        }

        let poll_result = Pin::new(&mut stream_wrapper).poll_flush(&mut cx);
        match poll_result {
            Poll::Ready(Ok(())) => {}
            _ => panic!("poll_flush for error stream should return Ready(Ok)"),
        }

        let poll_result = Pin::new(&mut stream_wrapper).poll_shutdown(&mut cx);
        match poll_result {
            Poll::Ready(Ok(())) => {}
            _ => panic!("poll_shutdown for error stream should return Ready(Ok)"),
        }
    }
}
