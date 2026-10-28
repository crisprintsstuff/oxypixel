//! The live MCP bridge: the open editor lends its document to `oxypixel mcp`.
//!
//! It is off until the user turns it on in Settings. Then the editor listens
//! on a random port on 127.0.0.1 and writes `bridge.txt` (the port and a
//! random token) into its data folder, readable only by the user on Linux.
//! The MCP server reads that file, so only a process of the same user can
//! use the bridge, and only while the editor runs with it on.
//!
//! A request is one line of JSON, then `len` bytes:
//!
//! ```text
//! {"token": "...", "op": "get"}
//! {"token": "...", "op": "put", "id": N, "len": L}   then L bytes of .oxypix
//! ```
//!
//! The reply is one line, then `len` bytes:
//!
//! ```text
//! {"ok": true, "id": N, "len": L}   then L bytes of .oxypix (get)
//! {"ok": true}                      (put)
//! {"ok": false, "error": "..."}
//! ```
//!
//! A put names the get it started from. The editor refuses it when the
//! document changed in the window since then, so an edit made by hand is
//! never overwritten.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use std::collections::VecDeque;

use oxypixel_core::{load_oxypix_bytes, save_oxypix_bytes, Document, Editor, MAX_FILE_BYTES};
use serde_json::{json, Value};

const FILE_NAME: &str = "bridge.txt";
/// How long either side waits for the other.
const TIMEOUT: Duration = Duration::from_secs(10);
/// Longest request or reply line.
const MAX_HEADER: u64 = 4096;

/// What the bridge thread asks the window for.
pub enum Request {
    Get(Sender<Reply>),
    Put {
        id: u64,
        bytes: Vec<u8>,
        reply: Sender<Reply>,
    },
}

pub enum Reply {
    Document { id: u64, bytes: Vec<u8> },
    Done,
    Error(String),
}

/// The listening side, owned by the window. Dropping it stops the thread
/// and removes `bridge.txt`.
pub struct BridgeServer {
    stop: Arc<AtomicBool>,
    requests: Receiver<Request>,
    thread: Option<JoinHandle<()>>,
    info: PathBuf,
}

impl BridgeServer {
    /// Listen and write `bridge.txt`. `wake` is called after each request
    /// is queued, so the window can answer it without waiting for input.
    pub fn start(wake: impl Fn() + Send + 'static) -> Result<Self, String> {
        let info = info_path().ok_or("there is no data folder for the bridge file")?;
        Self::start_at(info, wake)
    }

    /// [`Self::start`] with the bridge file at `info`.
    pub fn start_at(info: PathBuf, wake: impl Fn() + Send + 'static) -> Result<Self, String> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let token = new_token();
        write_private(&info, &format!("port={port}\ntoken={token}\n"))?;
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, requests) = mpsc::channel();
        let flag = stop.clone();
        let thread = std::thread::spawn(move || {
            while !flag.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        // One connection at a time; a broken one only ends itself.
                        let _ = serve(stream, &token, &tx, &wake);
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(50)),
                }
            }
        });
        Ok(Self {
            stop,
            requests,
            thread: Some(thread),
            info,
        })
    }

    /// The next request waiting for the window, if any.
    pub fn poll(&self) -> Option<Request> {
        self.requests.try_recv().ok()
    }
}

impl Drop for BridgeServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let _ = std::fs::remove_file(&self.info);
    }
}

/// Copies of the document lent out over the bridge, newest last.
#[derive(Default)]
pub struct Lent {
    next: u64,
    copies: VecDeque<(u64, Document)>,
}

/// Lent copies kept for a later put. An older put is refused.
const MAX_LENT: usize = 8;

impl Lent {
    /// Answer one request with the window's editor. `true` when an edit
    /// was applied to the document.
    pub fn answer(&mut self, request: Request, editor: &mut Editor) -> bool {
        match request {
            Request::Get(reply) => {
                let answer = match save_oxypix_bytes(editor.document()) {
                    Ok(bytes) => {
                        self.next += 1;
                        self.copies
                            .push_back((self.next, editor.document().clone()));
                        while self.copies.len() > MAX_LENT {
                            self.copies.pop_front();
                        }
                        Reply::Document {
                            id: self.next,
                            bytes,
                        }
                    }
                    Err(err) => Reply::Error(err.to_string()),
                };
                let _ = reply.send(answer);
                false
            }
            Request::Put { id, bytes, reply } => {
                let Some((_, lent)) = self.copies.iter().find(|(n, _)| *n == id) else {
                    let _ = reply.send(Reply::Error(
                        "that copy of the document is too old; read it again".into(),
                    ));
                    return false;
                };
                if !same_content(lent, editor.document()) {
                    let _ = reply.send(Reply::Error(
                        "the document was changed in the window after it was read; read it again and retry".into(),
                    ));
                    return false;
                }
                match load_oxypix_bytes(&bytes) {
                    Ok(doc) => {
                        editor.apply_external(doc);
                        // Every copy lent so far is now out of date.
                        self.copies.clear();
                        let _ = reply.send(Reply::Done);
                        true
                    }
                    Err(err) => {
                        let _ = reply.send(Reply::Error(err.to_string()));
                        false
                    }
                }
            }
        }
    }
}

/// The same document, whichever frame and layer each has active.
fn same_content(a: &Document, b: &Document) -> bool {
    let mut b = b.clone();
    let _ = b.set_active_frame(a.active_frame());
    let _ = b.set_active_layer(a.active_layer());
    *a == b
}

fn serve(
    stream: TcpStream,
    token: &str,
    requests: &Sender<Request>,
    wake: &impl Fn(),
) -> Result<(), String> {
    stream.set_nonblocking(false).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(TIMEOUT))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(TIMEOUT))
        .map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
    let header = read_header(&mut reader)?;
    let mut out = stream;
    let given = header.get("token").and_then(Value::as_str).unwrap_or("");
    if !same(given, token) {
        return send(
            &mut out,
            json!({ "ok": false, "error": "wrong token" }),
            &[],
        );
    }
    let (tx, rx) = mpsc::channel();
    let request = match header.get("op").and_then(Value::as_str) {
        Some("get") => Request::Get(tx),
        Some("put") => {
            let id = header.get("id").and_then(Value::as_u64).unwrap_or(0);
            let bytes = read_body(&mut reader, &header)?;
            Request::Put {
                id,
                bytes,
                reply: tx,
            }
        }
        _ => return send(&mut out, json!({ "ok": false, "error": "unknown op" }), &[]),
    };
    requests.send(request).map_err(|e| e.to_string())?;
    wake();
    match rx.recv_timeout(TIMEOUT) {
        Ok(Reply::Document { id, bytes }) => send(
            &mut out,
            json!({ "ok": true, "id": id, "len": bytes.len() }),
            &bytes,
        ),
        Ok(Reply::Done) => send(&mut out, json!({ "ok": true }), &[]),
        Ok(Reply::Error(error)) => send(&mut out, json!({ "ok": false, "error": error }), &[]),
        Err(_) => send(
            &mut out,
            json!({ "ok": false, "error": "OxyPixel did not answer in time" }),
            &[],
        ),
    }
}

/// The open document as `.oxypix` bytes, and the id to name when putting
/// it back. `info` is the bridge file the editor wrote.
pub fn fetch(info: Option<&Path>) -> Result<(u64, Vec<u8>), String> {
    let (header, bytes) = exchange(info, json!({ "op": "get" }), &[])?;
    let id = header
        .get("id")
        .and_then(Value::as_u64)
        .ok_or("the bridge sent no id")?;
    Ok((id, bytes))
}

/// Replace the open document with `bytes`, edited from the get named `id`.
pub fn push(info: Option<&Path>, id: u64, bytes: &[u8]) -> Result<(), String> {
    exchange(
        info,
        json!({ "op": "put", "id": id, "len": bytes.len() }),
        bytes,
    )
    .map(|_| ())
}

fn exchange(
    info: Option<&Path>,
    mut header: Value,
    body: &[u8],
) -> Result<(Value, Vec<u8>), String> {
    const OFF: &str = "OxyPixel is not open with the live bridge on. Turn it on in OxyPixel under Edit, then Settings…, Live MCP bridge, or pass a path to work on a file.";
    let info = info.ok_or(OFF)?;
    let text = std::fs::read_to_string(info).map_err(|_| OFF)?;
    let (port, token) = parse_info(&text).ok_or(OFF)?;
    header["token"] = Value::from(token);
    let mut stream = TcpStream::connect_timeout(&(Ipv4Addr::LOCALHOST, port).into(), TIMEOUT)
        .map_err(|_| OFF)?;
    stream
        .set_read_timeout(Some(TIMEOUT))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(TIMEOUT))
        .map_err(|e| e.to_string())?;
    send(&mut stream, header, body)?;
    let mut reader = BufReader::new(stream);
    let reply = read_header(&mut reader)?;
    if reply.get("ok").and_then(Value::as_bool) != Some(true) {
        let error = reply
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("the bridge refused the request");
        return Err(format!("OxyPixel: {error}"));
    }
    let bytes = read_body(&mut reader, &reply)?;
    Ok((reply, bytes))
}

fn send(stream: &mut TcpStream, header: Value, body: &[u8]) -> Result<(), String> {
    let mut line = header.to_string();
    line.push('\n');
    stream
        .write_all(line.as_bytes())
        .and_then(|()| stream.write_all(body))
        .and_then(|()| stream.flush())
        .map_err(|e| e.to_string())
}

fn read_header(reader: &mut impl BufRead) -> Result<Value, String> {
    let mut line = String::new();
    reader
        .by_ref()
        .take(MAX_HEADER)
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&line).map_err(|_| "the bridge message is not valid".to_string())
}

fn read_body(reader: &mut impl Read, header: &Value) -> Result<Vec<u8>, String> {
    let len = header.get("len").and_then(Value::as_u64).unwrap_or(0);
    if len > MAX_FILE_BYTES {
        return Err("the document is too large for the bridge".into());
    }
    let mut bytes = vec![0; len as usize];
    reader.read_exact(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes)
}

fn parse_info(text: &str) -> Option<(u16, String)> {
    let mut port = None;
    let mut token = None;
    for line in text.lines() {
        match line.split_once('=') {
            Some(("port", value)) => port = value.trim().parse().ok(),
            Some(("token", value)) => token = Some(value.trim().to_string()),
            _ => {}
        }
    }
    Some((port?, token.filter(|t| !t.is_empty())?))
}

/// Compare without stopping at the first difference.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

/// Where the editor writes the bridge file.
pub fn info_path() -> Option<PathBuf> {
    crate::recent::data_dir().map(|dir| dir.join(FILE_NAME))
}

/// 32 hex digits, different on each call.
fn new_token() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let random = RandomState::new();
    let mut first = random.build_hasher();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    first.write_u128(nanos);
    first.write_u32(std::process::id());
    let a = first.finish();
    let mut second = RandomState::new().build_hasher();
    second.write_u64(a);
    format!("{a:016x}{:016x}", second.finish())
}

/// Write a file only its owner can read, where the platform allows it.
fn write_private(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|e| e.to_string())?;
    file.write_all(text.as_bytes()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_put_after_an_edit_in_the_window_is_refused() {
        let mut editor = Editor::new(2, 2).unwrap();
        let mut lent = Lent::default();
        let (tx, rx) = mpsc::channel();
        lent.answer(Request::Get(tx), &mut editor);
        let Ok(Reply::Document { id, bytes }) = rx.recv() else {
            panic!("no document");
        };
        // Switching frames in the window is not an edit.
        editor.add_frame().unwrap();
        editor.undo();
        let (tx, rx) = mpsc::channel();
        assert!(lent.answer(
            Request::Put {
                id,
                bytes: bytes.clone(),
                reply: tx
            },
            &mut editor
        ));
        assert!(matches!(rx.recv(), Ok(Reply::Done)));
        // That copy is used up; the next put needs a new get.
        let (tx, rx) = mpsc::channel();
        assert!(!lent.answer(
            Request::Put {
                id,
                bytes: bytes.clone(),
                reply: tx
            },
            &mut editor
        ));
        assert!(matches!(rx.recv(), Ok(Reply::Error(_))));
        // An edit by hand after the get refuses the put.
        let (tx, rx) = mpsc::channel();
        lent.answer(Request::Get(tx), &mut editor);
        let Ok(Reply::Document { id, bytes }) = rx.recv() else {
            panic!("no document");
        };
        editor.pointer_down(0, 0, oxypixel_core::MouseButton::Primary, false);
        editor.pointer_up(0, 0, false);
        let (tx, rx) = mpsc::channel();
        assert!(!lent.answer(
            Request::Put {
                id,
                bytes,
                reply: tx
            },
            &mut editor
        ));
        let Ok(Reply::Error(message)) = rx.recv() else {
            panic!("the put went through");
        };
        assert!(message.contains("changed in the window"));
    }

    #[test]
    fn a_wrong_token_is_refused() {
        let dir =
            std::env::temp_dir().join(format!("oxypixel-bridge-token-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let info = dir.join("bridge.txt");
        let server = BridgeServer::start_at(info.clone(), || {}).unwrap();
        let text = std::fs::read_to_string(&info).unwrap();
        let (port, _) = parse_info(&text).unwrap();
        std::fs::write(&info, format!("port={port}\ntoken=0000\n")).unwrap();
        let err = fetch(Some(&info)).unwrap_err();
        assert!(err.contains("wrong token"), "{err}");
        assert!(server.poll().is_none(), "nothing reached the window");
        drop(server);
        assert!(!info.exists(), "stopping removes the bridge file");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn the_bridge_file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("oxypixel-bridge-mode-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let info = dir.join("bridge.txt");
        let server = BridgeServer::start_at(info.clone(), || {}).unwrap();
        let mode = std::fs::metadata(&info).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        drop(server);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn info_lines_and_tokens() {
        assert_eq!(
            parse_info("port=4100\ntoken=abc\n"),
            Some((4100, "abc".to_string()))
        );
        assert_eq!(parse_info("port=x\ntoken=abc\n"), None);
        assert_eq!(parse_info("port=1\ntoken=\n"), None);
        assert!(same("abc", "abc"));
        assert!(!same("abc", "abd") && !same("abc", "ab"));
        let (a, b) = (new_token(), new_token());
        assert_eq!(a.len(), 32);
        assert_ne!(a, b);
    }
}
