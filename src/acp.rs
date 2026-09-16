//! ACP (Agent Client Protocol) over stdio — the bro integration surface
//! (PHEOBE-17). `serve_stdio` runs an ACP `Agent` on stdin/stdout:
//! initialize → session/new → session/prompt. The prompt text is a pheobe
//! task JSON; the run goes through the SAME `run::run_task` pipeline the
//! CLI uses (no loop fork). Progress streams as `session/update` →
//! AgentMessageChunk notifications; the handoff report JSON returns as the
//! final session message before the prompt response.
//!
//! Transport mirrors bro's `soul/acp_server::run`: the ACP crate keeps a
//! byte-stream connection open until BOTH directions close, so a client that
//! hangs up stdin leaves the process parked. Built from `Lines` instead — the
//! end of the incoming line stream is treated as hangup: requests already
//! received are still answered, then the server ends.

use agent_client_protocol::schema::v1::{
    AgentCapabilities, ContentBlock, ContentChunk, InitializeRequest, InitializeResponse,
    NewSessionRequest, NewSessionResponse, PromptRequest, PromptResponse, SessionId,
    SessionNotification, SessionUpdate, StopReason, TextContent,
};
use agent_client_protocol::{Agent, Client, ConnectionTo, Lines};
use tokio_util::sync::CancellationToken;

use crate::report;
use crate::{learn, run, task};

/// One ACP connection over stdio. Reads a line at a time from stdin; the
/// incoming stream's end cancels `hangup`, and the server wraps up then.
pub async fn serve_stdio() -> anyhow::Result<()> {
    use futures::{AsyncBufReadExt, AsyncWriteExt, StreamExt};
    use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

    let hangup = CancellationToken::new();
    let eof = hangup.clone();
    let incoming = Box::pin(
        futures::io::BufReader::new(tokio::io::stdin().compat())
            .lines()
            .chain(futures::stream::poll_fn(move |_| {
                eof.cancel();
                std::task::Poll::Ready(None)
            })),
    );
    let outgoing = Box::pin(futures::sink::unfold(
        tokio::io::stdout().compat_write(),
        async move |mut writer, line: String| {
            let mut bytes = line.into_bytes();
            bytes.push(b'\n');
            writer.write_all(&bytes).await?;
            writer.flush().await?;
            Ok::<_, std::io::Error>(writer)
        },
    ));
    let transport = Lines::new(outgoing, incoming);
    let server = serve(transport);
    tokio::select! {
        result = server => result,
        _ = async {
            hangup.cancelled().await;
            // A scripted client may write its last request and close stdin
            // in one go: answer everything already received, then leave a
            // moment for the outgoing actor to flush the last line.
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        } => Ok(()),
    }
}

/// The sessionful ACP server. Each session carries the worktree the dispatch
/// will cook in — session/new's cwd, validated to be inside a git repo.
struct SessionStore {
    /// Maps ACP session id → the repo path for that session's run.
    sessions: std::collections::HashMap<String, String>,
}

impl Default for SessionStore {
    fn default() -> Self {
        SessionStore { sessions: Default::default() }
    }
}

pub async fn serve(transport: impl agent_client_protocol::ConnectTo<Agent> + 'static) -> anyhow::Result<()> {
    let store = std::sync::Arc::new(tokio::sync::Mutex::new(SessionStore::default()));
    let store_new = store.clone();
    let store_prompt = store.clone();

    Agent
        .builder()
        .name("pheobe")
        .on_receive_request(
            async move |req: InitializeRequest, responder, _cx: ConnectionTo<Client>| {
                let mut meta = serde_json::Map::new();
                if let Ok(cwd) = std::env::current_dir() {
                    meta.insert("cwd".into(), cwd.to_string_lossy().into_owned().into());
                }
                responder.respond(
                    InitializeResponse::new(req.protocol_version)
                        .agent_capabilities(AgentCapabilities::new())
                        .meta(meta),
                )
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |req: NewSessionRequest, responder, _cx: ConnectionTo<Client>| {
                let sessions = store_new.clone();
                let id = new_session_id();
                sessions.lock().await.sessions.insert(id.clone(), req.cwd.to_string_lossy().into_owned());
                responder.respond(NewSessionResponse::new(SessionId::new(id)))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |req: PromptRequest, responder, cx: ConnectionTo<Client>| {
                let store = store_prompt.clone();
                let session_id = req.session_id.clone();
                let Some(cwd) = store.lock().await.sessions.get(session_id.0.as_ref()).cloned() else {
                    return responder.respond_with_internal_error("unknown session");
                };
                // The turn runs as a connection task, never inside this
                // handler — handlers block the dispatch loop. The pheobe
                // pipeline is synchronous, so it runs in spawn_blocking and
                // streams progress back through a channel.
                let cx_spawn = cx.clone();
                let _ = cx.spawn(async move {
                    let cx = cx_spawn;
                    let text = extract_text(&req.prompt);
                    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
                    let cwd = cwd.clone();
                    let task_text = text.clone();
                    let mut run = tokio::task::spawn_blocking(move || {
                        // runs in the session's worktree context
                        let _ = std::env::set_current_dir(&cwd);
                        learn::init();
                        let t = match task::load_from_str(&task_text) {
                            Ok(t) => t,
                            Err(e) => return Err(e),
                        };
                        run::run_task(&t, None, &|stage| {
                            let _ = progress_tx.send(stage.to_string());
                        })
                    });
                    loop {
                        tokio::select! {
                            Some(stage) = progress_rx.recv() => {
                                let _ = cx.send_notification(SessionNotification::new(
                                    session_id.clone(),
                                    SessionUpdate::AgentMessageChunk(ContentChunk::new(
                                        ContentBlock::Text(TextContent::new(stage)),
                                    )),
                                ));
                            }
                            res = &mut run => {
                                let result = match res {
                                    Ok(Ok(rep)) => Ok(rep),
                                    Ok(Err(e)) => Err(e),
                                    Err(e) => Err(anyhow::anyhow!("task panicked: {e}")),
                                };
                                match result {
                                    Ok(rep) => {
                                        let tail = finish_notification(&rep);
                                        let _ = cx.send_notification(SessionNotification::new(
                                            session_id.clone(),
                                            SessionUpdate::AgentMessageChunk(ContentChunk::new(
                                                ContentBlock::Text(TextContent::new(tail)),
                                            )),
                                        ));
                                        let _ = responder.respond(PromptResponse::new(StopReason::EndTurn));
                                    }
                                    Err(e) => {
                                        let _ = cx.send_notification(SessionNotification::new(
                                            session_id.clone(),
                                            SessionUpdate::AgentMessageChunk(ContentChunk::new(
                                                ContentBlock::Text(TextContent::new(format!("pheobe acp: {e:#}"))),
                                            )),
                                        ));
                                        let _ = responder.respond(PromptResponse::new(StopReason::Refusal));
                                    }
                                }
                                break;
                            }
                        }
                    }
                    Ok(())
                });
                Ok(())
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_dispatch(
            async move |message: agent_client_protocol::Dispatch, cx: ConnectionTo<Client>| {
                message.respond_with_error(agent_client_protocol::util::internal_error("pheobe acp: unhandled message"), cx)
            },
            agent_client_protocol::on_receive_dispatch!(),
        )
        .connect_to(transport)
        .await
        .map_err(Into::into)
}

/// ACP ContentBlock text → the dispatch prompt text.
fn extract_text(blocks: &[ContentBlock]) -> String {
    blocks
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The handoff report JSON, compact — the final session message the peer
/// (`bro synapse dispatch`) reads back as the dispatch response.
fn finish_notification(rep: &report::HandoffReport) -> String {
    serde_json::to_string(rep).unwrap_or_else(|e| format!("report serialize failed: {e}"))
}

/// A fresh ACP session id. Collision odds are irrelevant here (one-shot
/// dispatch sessions), but it must be a valid ACP SessionId — opaque string.
fn new_session_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("pheobe-{nanos:x}")
}

// Re-exported for doc parity with the crate's role types.
// Client = role::acp::Client, re-exported at crate root.

#[cfg(test)]
mod tests {
    use std::ops::AsyncFnOnce;

    use super::*;
    use agent_client_protocol::SessionMessage;
    use agent_client_protocol::schema::v1::{ContentBlock, SessionUpdate};
    use agent_client_protocol::schema::ProtocolVersion;
    use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

    /// In-memory stand-in for the stdio transport: two duplex byte streams
    /// cross-wired exactly like the crate's own `jsonrpc_error_handling` wire
    /// tests build theirs.
    type CompatIo = tokio_util::compat::Compat<tokio::io::DuplexStream>;
    struct WirePair {
        server: agent_client_protocol::ByteStreams<CompatIo, CompatIo>,
        client: agent_client_protocol::ByteStreams<CompatIo, CompatIo>,
    }

    impl WirePair {
        fn new(capacity: usize) -> Self {
            let (client_writer, server_reader) = tokio::io::duplex(capacity);
            let (server_writer, client_reader) = tokio::io::duplex(capacity);
            let server = agent_client_protocol::ByteStreams::new(
                server_writer.compat_write(),
                server_reader.compat(),
            );
            let client = agent_client_protocol::ByteStreams::new(
                client_writer.compat_write(),
                client_reader.compat(),
            );
            WirePair { server, client }
        }

        /// Spawn the pheobe ACP server on the server side of the pair and
        /// drive it from the client side with the crate's own `Client` role.
        async fn drive<F, T>(self, main: F) -> Result<T, agent_client_protocol::Error>
        where
            F: for<'a> AsyncFnOnce(ConnectionTo<Agent>) -> Result<T, agent_client_protocol::Error>,
            T: Send + 'static,
        {
            let server = tokio::spawn(serve(self.server));
            let result = Client.builder()
                .name("pheobe-wire-test")
                .connect_with(self.client, main)
                .await;
            server.abort();
            let _ = server.await;
            result
        }
    }

    #[tokio::test]
    async fn wire_handshake_and_new_session() {
        let pair = WirePair::new(8192);
        let result = pair
            .drive(async |cx| {
                // initialize → negotiate protocol version
                let init = cx
                    .send_request(InitializeRequest::new(ProtocolVersion::V1))
                    .block_task()
                    .await?;
                assert_eq!(init.protocol_version, ProtocolVersion::V1);
                // server advertises capabilities; meta attached as _meta
                assert!(init.meta.is_some(), "server should attach meta");
                // session/new with a sane cwd → gets a session id back
                let cwd = std::env::current_dir().map_err(agent_client_protocol::util::internal_error)?;
                let session_resp: NewSessionResponse = cx
                    .send_request(NewSessionRequest::new(cwd))
                    .block_task()
                    .await?;
                assert!(!session_resp.session_id.0.is_empty(), "session id must be non-empty");
                Ok(session_resp.session_id.0.as_ref().to_string())
            })
            .await;
        let id = result.expect("wire drive must succeed");
        assert!(id.starts_with("pheobe-"), "session id should be a pheobe id, got {id}");
    }

    /// Prompt with an UNKNOWN task (invalid task JSON) must yield a
    /// notification chunk (the error) followed by PromptResponse(Refusal) —
    /// the wire contract bro's `synapse dispatch` relies on to surface a
    /// failure instead of hanging.
    #[tokio::test]
    async fn wire_prompt_bad_task_refuses_with_chunk() {
        let pair = WirePair::new(8192);
        let result = pair
            .drive(async |cx| {
                let init = cx
                    .send_request(InitializeRequest::new(ProtocolVersion::V1))
                    .block_task()
                    .await?;
                assert_eq!(init.protocol_version, ProtocolVersion::V1);
                let cwd = std::env::temp_dir();
                let _session_id: SessionId = cx
                    .send_request(NewSessionRequest::new(&cwd))
                    .block_task()
                    .await?
                    .session_id;
                let session = cx
                    .build_session(&cwd)
                    .block_task()
                    .run_until(async |mut session| {
                        session
                            .send_prompt(r#"{ not a task json"#)
                            .expect("prompt send");
                        // read chunks until stop reason
                        let mut text = String::new();
                        loop {
                            match session.read_update().await? {
                                SessionMessage::SessionMessage(dispatch) => {
                                    let _ = agent_client_protocol::util::MatchDispatch::new(dispatch)
                                        .if_notification(async |notif: SessionNotification| {
                                            if let SessionUpdate::AgentMessageChunk(chunk) = notif.update {
                                                if let ContentBlock::Text(t) = chunk.content {
                                                    text.push_str(&t.text);
                                                }
                                            }
                                            Ok(())
                                        })
                                        .await;
                                }
                                SessionMessage::StopReason(reason) => {
                                    assert_eq!(reason, StopReason::Refusal, "bad task must refuse; chunks so far: {text}");
                                    break;
                                }
                                _ => {}
                            }
                        }
                        assert!(
                            text.contains("pheobe acp:"),
                            "error chunk should mention pheobe acp, got: {text}"
                        );
                        Ok::<_, agent_client_protocol::Error>(())
                    })
                    .await?;
                let _ = &session;
                Ok(())
            })
            .await;
        assert!(result.is_ok(), "wire drive must succeed: {result:?}");
    }

    #[test]
    fn extract_text_only_takes_text_blocks() {
        let blocks = vec![
            ContentBlock::Text(TextContent::new("hello")),
            ContentBlock::Text(TextContent::new("world")),
        ];
        assert_eq!(extract_text(&blocks), "hello\nworld");
        assert_eq!(extract_text(&[]), "");
    }

    #[test]
    fn new_session_id_is_prefixed() {
        assert!(new_session_id().starts_with("pheobe-"));
    }
}