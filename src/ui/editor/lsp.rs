//! The editor's side of the Piton language server.
//!
//! The server is the one the language ships, run as `piton lsp` and spoken to
//! over its standard input and output in the usual JSON-RPC framing. It knows
//! what the grammar alone cannot: whether a name resolves, what a declaration
//! ended up holding, where a problem is.
//!
//! Reading and writing happen on threads of their own, so a server that is
//! slow, or gone, never holds a frame up: the editor hands over what the buffer
//! says and takes whatever has arrived since the last frame.
//!
//! The transport is a trait so the tests can put a server of their own behind
//! it and drive the whole client without starting a process.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use serde_json::{json, Value};

/// The program the editor runs to get a language server.
pub const SERVER_COMMAND: &str = "piton";
pub const SERVER_ARGS: &[&str] = &["lsp", "--stdio"];

// ---------------------------------------------------------------- transport

/// A way of exchanging JSON-RPC messages with a language server.
pub trait Transport: Send {
    /// Hand one message over. Never blocks.
    fn send(&mut self, message: Value);

    /// Everything that has arrived since the last call. Never blocks.
    fn receive(&mut self) -> Vec<Value>;

    /// False once the server has gone.
    fn alive(&self) -> bool;
}

/// A language server running as a child process.
struct ChildProcess {
    child: Child,
    outgoing: Sender<Value>,
    incoming: Receiver<Value>,
    alive: Arc<AtomicBool>,
}

impl ChildProcess {
    fn spawn() -> std::io::Result<ChildProcess> {
        let mut child = Command::new(SERVER_COMMAND)
            .args(SERVER_ARGS)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdin = child.stdin.take().expect("the child was given a pipe");
        let stdout = child.stdout.take().expect("the child was given a pipe");
        let alive = Arc::new(AtomicBool::new(true));

        let (outgoing, to_write) = channel::<Value>();
        let writing = alive.clone();
        std::thread::spawn(move || write_messages(stdin, to_write, &writing));

        let (from_read, incoming) = channel::<Value>();
        let reading = alive.clone();
        std::thread::spawn(move || {
            read_messages(BufReader::new(stdout), &from_read);
            reading.store(false, Ordering::Relaxed);
        });

        Ok(ChildProcess {
            child,
            outgoing,
            incoming,
            alive,
        })
    }
}

impl Transport for ChildProcess {
    fn send(&mut self, message: Value) {
        if self.outgoing.send(message).is_err() {
            self.alive.store(false, Ordering::Relaxed);
        }
    }

    fn receive(&mut self) -> Vec<Value> {
        let mut out = Vec::new();
        loop {
            match self.incoming.try_recv() {
                Ok(message) => out.push(message),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.alive.store(false, Ordering::Relaxed);
                    break;
                }
            }
        }
        out
    }

    fn alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }
}

impl Drop for ChildProcess {
    fn drop(&mut self) {
        // The server outlives its pipes otherwise, and nothing would ever read
        // from it again.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn write_messages(mut stdin: impl Write, messages: Receiver<Value>, alive: &AtomicBool) {
    while let Ok(message) = messages.recv() {
        let body = message.to_string();
        if write!(stdin, "Content-Length: {}\r\n\r\n{}", body.len(), body).is_err()
            || stdin.flush().is_err()
        {
            break;
        }
    }
    alive.store(false, Ordering::Relaxed);
}

fn read_messages(mut stdout: impl BufRead, messages: &Sender<Value>) {
    loop {
        let mut length = None;
        // The header block: `Content-Length`, maybe others, then a blank line.
        loop {
            let mut line = String::new();
            match stdout.read_line(&mut line) {
                Ok(0) | Err(_) => return,
                Ok(_) => {}
            }
            let trimmed = line.trim_end_matches(['\r', '\n']);
            if trimmed.is_empty() {
                break;
            }
            if let Some(value) = trimmed.strip_prefix("Content-Length:") {
                length = value.trim().parse::<usize>().ok();
            }
        }
        let Some(length) = length else { return };
        let mut body = vec![0u8; length];
        if stdout.read_exact(&mut body).is_err() {
            return;
        }
        match serde_json::from_slice(&body) {
            Ok(message) => {
                if messages.send(message).is_err() {
                    return;
                }
            }
            // One unreadable message is not worth dropping the server for.
            Err(_) => continue,
        }
    }
}

// ------------------------------------------------------------------ problems

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

impl Severity {
    fn from_code(code: u64) -> Severity {
        match code {
            1 => Severity::Error,
            2 => Severity::Warning,
            _ => Severity::Note,
        }
    }
}

/// One thing the server has to say about a file.
#[derive(Debug, Clone)]
pub struct Problem {
    pub severity: Severity,
    pub message: String,
    /// Where it is, in the line and UTF-16 character the server counts in.
    pub start: Position,
    pub end: Position,
}

impl Problem {
    /// Where the problem falls in `text`, as a byte range.
    pub fn range_in(&self, text: &str) -> Range<usize> {
        let start = offset_of(text, self.start);
        let end = offset_of(text, self.end).max(start);
        // A problem the server points at with an empty range still has to be
        // visible, so it takes the character it sits on.
        if start == end {
            start..text.len().min(start + 1)
        } else {
            start..end
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

/// How the server is getting on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Spoken to, and waiting for it to answer.
    Starting,
    /// Answering.
    Ready,
    /// There is none, and why.
    Unavailable(String),
}

// -------------------------------------------------------------------- client

/// What the editor knows about one file the server has been told about.
struct Document {
    version: i64,
    text: String,
}

/// The client: one language server, and what it has said so far.
pub struct Lsp {
    transport: Box<dyn Transport>,
    root: PathBuf,
    status: Status,
    next_id: i64,
    documents: HashMap<PathBuf, Document>,
    problems: HashMap<PathBuf, Vec<Problem>>,
    /// The hover the editor is waiting on, and the last one it was given.
    asked: Option<(PathBuf, Position, i64)>,
    answer: Option<(PathBuf, Position, String)>,
}

impl Lsp {
    /// Start the language server for a project.
    ///
    /// A missing server is not a failure the editor stops for: the file still
    /// opens, and still highlights, with the status saying what is missing.
    pub fn start(root: &Path) -> Lsp {
        match ChildProcess::spawn() {
            Ok(transport) => Lsp::new(Box::new(transport), root),
            Err(error) => Lsp {
                transport: Box::new(Silent),
                root: root.to_path_buf(),
                status: Status::Unavailable(format!("{SERVER_COMMAND}: {error}")),
                next_id: 1,
                documents: HashMap::new(),
                problems: HashMap::new(),
                asked: None,
                answer: None,
            },
        }
    }

    /// A client over a transport of your own, already spoken to.
    pub fn new(transport: Box<dyn Transport>, root: &Path) -> Lsp {
        let mut lsp = Lsp {
            transport,
            root: root.to_path_buf(),
            status: Status::Starting,
            next_id: 1,
            documents: HashMap::new(),
            problems: HashMap::new(),
            asked: None,
            answer: None,
        };
        let id = lsp.next_id();
        let root_uri = uri(&lsp.root);
        lsp.transport.send(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "initialize",
            "params": {
                "processId": std::process::id(),
                "rootUri": root_uri,
                "capabilities": {
                    "textDocument": {
                        "synchronization": { "didSave": true },
                        "publishDiagnostics": {},
                        "hover": { "contentFormat": ["markdown", "plaintext"] },
                    }
                },
            }
        }));
        lsp
    }

    pub fn status(&self) -> &Status {
        &self.status
    }

    /// The project the server was started for.
    #[allow(dead_code)] // The tests read the session through this.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Take in everything the server has said since the last frame.
    pub fn poll(&mut self) {
        for message in self.transport.receive() {
            self.handle(message);
        }
        if !self.transport.alive() && !matches!(self.status, Status::Unavailable(_)) {
            self.status = Status::Unavailable(format!("{SERVER_COMMAND} stopped"));
        }
    }

    /// Tell the server what a file holds now, opening it if it is new to it.
    ///
    /// A file the editor opens before the server has finished starting is kept
    /// until it has; the server takes nothing before it has answered.
    pub fn document(&mut self, path: &Path, text: &str) {
        let uri = uri(path);
        let waiting = self.status != Status::Ready;
        match self.documents.get_mut(path) {
            Some(document) => {
                if document.text == text {
                    return;
                }
                document.text = text.to_string();
                if waiting {
                    return;
                }
                document.version += 1;
                let version = document.version;
                self.transport.send(json!({
                    "jsonrpc": "2.0",
                    "method": "textDocument/didChange",
                    "params": {
                        "textDocument": { "uri": uri, "version": version },
                        // The server asked for whole documents, not edits.
                        "contentChanges": [{ "text": text }],
                    }
                }));
            }
            None => {
                self.documents.insert(
                    path.to_path_buf(),
                    Document {
                        version: 1,
                        text: text.to_string(),
                    },
                );
                if waiting {
                    return;
                }
                self.transport.send(json!({
                    "jsonrpc": "2.0",
                    "method": "textDocument/didOpen",
                    "params": {
                        "textDocument": {
                            "uri": uri,
                            "languageId": "piton",
                            "version": 1,
                            "text": text,
                        }
                    }
                }));
            }
        }
    }

    /// Tell the server the editor is done with a file.
    pub fn close(&mut self, path: &Path) {
        if self.documents.remove(path).is_none() {
            return;
        }
        self.transport.send(json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didClose",
            "params": { "textDocument": { "uri": uri(path) } }
        }));
        if self.asked.as_ref().is_some_and(|(asked, _, _)| asked == path) {
            self.asked = None;
        }
        if self.answer.as_ref().is_some_and(|(at, _, _)| at == path) {
            self.answer = None;
        }
    }

    /// Tell the server a file was written, so it re-reads what is on disk.
    pub fn saved(&mut self, path: &Path, text: &str) {
        if !self.documents.contains_key(path) {
            return;
        }
        self.transport.send(json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didSave",
            "params": { "textDocument": { "uri": uri(path) }, "text": text }
        }));
    }

    /// What the server has said about a file.
    pub fn problems(&self, path: &Path) -> &[Problem] {
        self.problems.get(path).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Ask what is under a position, unless that has already been asked.
    pub fn ask_hover(&mut self, path: &Path, at: Position) {
        if self.status != Status::Ready {
            return;
        }
        let asked_already = self
            .asked
            .as_ref()
            .is_some_and(|(asked, position, _)| asked == path && *position == at);
        let answered_already = self
            .answer
            .as_ref()
            .is_some_and(|(answered, position, _)| answered == path && *position == at);
        if asked_already || answered_already {
            return;
        }
        let id = self.next_id();
        self.asked = Some((path.to_path_buf(), at, id));
        self.transport.send(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "textDocument/hover",
            "params": {
                "textDocument": { "uri": uri(path) },
                "position": { "line": at.line, "character": at.character },
            }
        }));
    }

    /// What the server said is at a position, when it has said anything.
    pub fn hover(&self, path: &Path, at: Position) -> Option<&str> {
        self.answer
            .as_ref()
            .filter(|(answered, position, _)| answered == path && *position == at)
            .map(|(_, _, text)| text.as_str())
    }

    // ------------------------------------------------------------ messages

    fn handle(&mut self, message: Value) {
        match (message.get("method"), message.get("id")) {
            // A request from the server. The only one this client is asked is
            // to register a capability, and an answer of nothing is an answer.
            (Some(_), Some(id)) => {
                let id = id.clone();
                self.transport
                    .send(json!({ "jsonrpc": "2.0", "id": id, "result": null }));
            }
            (Some(method), None) => self.notification(method.as_str().unwrap_or_default(), &message),
            (None, Some(id)) => self.response(id.as_i64().unwrap_or(-1), &message),
            (None, None) => {}
        }
    }

    fn notification(&mut self, method: &str, message: &Value) {
        if method != "textDocument/publishDiagnostics" {
            return;
        }
        let params = &message["params"];
        let Some(path) = params["uri"].as_str().and_then(path_of) else {
            return;
        };
        let problems = params["diagnostics"]
            .as_array()
            .map(|list| list.iter().map(problem).collect())
            .unwrap_or_default();
        self.problems.insert(path, problems);
    }

    fn response(&mut self, id: i64, message: &Value) {
        if id == 1 {
            // The answer to `initialize`; the server waits for the reply to it
            // before it will take anything else.
            self.status = Status::Ready;
            self.transport.send(json!({
                "jsonrpc": "2.0",
                "method": "initialized",
                "params": {},
            }));
            // Anything opened while it was starting still has to be sent.
            let opened: Vec<(PathBuf, String)> = self
                .documents
                .iter()
                .map(|(path, document)| (path.clone(), document.text.clone()))
                .collect();
            self.documents.clear();
            for (path, text) in opened {
                self.document(&path, &text);
            }
            return;
        }

        let Some((path, at, asked)) = self.asked.clone() else {
            return;
        };
        if asked != id {
            return;
        }
        self.asked = None;
        let contents = &message["result"]["contents"];
        let text = contents["value"]
            .as_str()
            .or_else(|| contents.as_str())
            .unwrap_or_default();
        let text = plain_text(text);
        self.answer = (!text.is_empty()).then_some((path, at, text));
    }

    fn next_id(&mut self) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

impl Drop for Lsp {
    fn drop(&mut self) {
        if self.status == Status::Ready {
            let id = self.next_id();
            self.transport
                .send(json!({ "jsonrpc": "2.0", "id": id, "method": "shutdown" }));
            self.transport
                .send(json!({ "jsonrpc": "2.0", "method": "exit" }));
        }
    }
}

/// The transport of a client that has no server: it swallows what it is given.
struct Silent;

impl Transport for Silent {
    fn send(&mut self, _message: Value) {}

    fn receive(&mut self) -> Vec<Value> {
        Vec::new()
    }

    fn alive(&self) -> bool {
        false
    }
}

// ------------------------------------------------------------------ reading

fn problem(value: &Value) -> Problem {
    let position = |at: &Value| Position {
        line: at["line"].as_u64().unwrap_or(0) as u32,
        character: at["character"].as_u64().unwrap_or(0) as u32,
    };
    Problem {
        severity: Severity::from_code(value["severity"].as_u64().unwrap_or(1)),
        message: value["message"].as_str().unwrap_or_default().to_string(),
        start: position(&value["range"]["start"]),
        end: position(&value["range"]["end"]),
    }
}

/// A `file://` URI for a path.
pub fn uri(path: &Path) -> String {
    let mut out = String::from("file://");
    for byte in path.to_string_lossy().bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// The path a `file://` URI points at.
fn path_of(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let mut out = Vec::new();
    let mut bytes = rest.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let hex: String = bytes.by_ref().take(2).map(char::from).collect();
            match u8::from_str_radix(&hex, 16) {
                Ok(byte) => out.push(byte),
                Err(_) => return None,
            }
        } else {
            out.push(byte);
        }
    }
    Some(PathBuf::from(String::from_utf8(out).ok()?))
}

/// The byte offset of a line and UTF-16 character in `text`.
pub fn offset_of(text: &str, at: Position) -> usize {
    let mut offset = 0;
    for _ in 0..at.line {
        match text[offset..].find('\n') {
            Some(newline) => offset += newline + 1,
            None => return text.len(),
        }
    }
    let line = &text[offset..];
    let line = &line[..line.find('\n').unwrap_or(line.len())];
    let mut characters = 0;
    for (index, character) in line.char_indices() {
        if characters >= at.character {
            return offset + index;
        }
        characters += character.len_utf16() as u32;
    }
    offset + line.len()
}

/// The line and UTF-16 character a byte offset falls on.
pub fn position_of(text: &str, offset: usize) -> Position {
    let offset = offset.min(text.len());
    let before = &text[..offset];
    let line = before.matches('\n').count() as u32;
    let start = before.rfind('\n').map(|at| at + 1).unwrap_or(0);
    Position {
        line,
        character: text[start..offset].encode_utf16().count() as u32,
    }
}

/// Markdown as a tooltip reads it: the words, without the marks around them.
fn plain_text(markdown: &str) -> String {
    let mut out = String::new();
    for line in markdown.lines() {
        if line.trim_start().starts_with("```") {
            continue;
        }
        let line = line.replace("**", "").replace('`', "");
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out.trim().to_string()
}

// --------------------------------------------------------------------- tests

/// A language server the tests answer for, in place of a running one.
#[cfg(test)]
pub mod fake {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Shared {
        /// Everything the client has sent.
        sent: Vec<Value>,
        /// Everything waiting for the client to take.
        pending: Vec<Value>,
        /// What the next hover is answered with.
        hover: Option<String>,
        alive: bool,
    }

    /// The tests' hold on the server: what it was told, and what it says next.
    #[derive(Clone)]
    pub struct Server(Arc<Mutex<Shared>>);

    struct Wire(Arc<Mutex<Shared>>);

    impl Server {
        /// A client talking to a scripted server, and the hold on that server.
        pub fn start(root: &Path) -> (Lsp, Server) {
            let shared = Arc::new(Mutex::new(Shared {
                alive: true,
                ..Shared::default()
            }));
            let lsp = Lsp::new(Box::new(Wire(shared.clone())), root);
            (lsp, Server(shared))
        }

        /// Every message the client has sent.
        pub fn sent(&self) -> Vec<Value> {
            self.0.lock().unwrap().sent.clone()
        }

        /// The messages the client has sent by one method.
        pub fn sent_by(&self, method: &str) -> Vec<Value> {
            self.sent()
                .into_iter()
                .filter(|message| message["method"] == method)
                .collect()
        }

        /// Say something to the client, as the server would.
        pub fn say(&self, message: Value) {
            self.0.lock().unwrap().pending.push(message);
        }

        /// Report problems with a file.
        pub fn publish(&self, path: &Path, diagnostics: Value) {
            self.say(json!({
                "jsonrpc": "2.0",
                "method": "textDocument/publishDiagnostics",
                "params": { "uri": uri(path), "diagnostics": diagnostics },
            }));
        }

        /// Answer the next hover with this, until it is set again.
        pub fn set_hover(&self, markdown: &str) {
            self.0.lock().unwrap().hover = Some(markdown.to_string());
        }

        /// Stop answering, the way a server that died stops answering.
        pub fn stop(&self) {
            self.0.lock().unwrap().alive = false;
        }
    }

    impl Transport for Wire {
        fn send(&mut self, message: Value) {
            let mut shared = self.0.lock().unwrap();
            shared.sent.push(message.clone());
            let Some(id) = message.get("id").and_then(Value::as_i64) else {
                return;
            };
            // The server answers what it is asked, as the real one does.
            match message["method"].as_str() {
                Some("initialize") => shared.pending.push(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": { "capabilities": { "hoverProvider": true } },
                })),
                Some("textDocument/hover") => {
                    let hover = shared.hover.clone();
                    shared.pending.push(match hover {
                        Some(markdown) => json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": { "contents": { "kind": "markdown", "value": markdown } },
                        }),
                        None => json!({ "jsonrpc": "2.0", "id": id, "result": null }),
                    });
                }
                _ => {}
            }
        }

        fn receive(&mut self) -> Vec<Value> {
            std::mem::take(&mut self.0.lock().unwrap().pending)
        }

        fn alive(&self) -> bool {
            self.0.lock().unwrap().alive
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::Server;
    use super::*;

    fn project() -> PathBuf {
        PathBuf::from("/tmp/overlook-lsp")
    }

    fn file() -> PathBuf {
        project().join("spec/Thing.pi")
    }

    /// A client that has finished starting up.
    fn ready() -> (Lsp, Server) {
        let (mut lsp, server) = Server::start(&project());
        lsp.poll();
        assert_eq!(lsp.status(), &Status::Ready);
        (lsp, server)
    }

    #[test]
    fn the_client_starts_the_conversation_the_server_expects() {
        let (mut lsp, server) = Server::start(&project());
        assert_eq!(lsp.status(), &Status::Starting);

        let initialize = &server.sent()[0];
        assert_eq!(initialize["method"], "initialize");
        assert_eq!(initialize["params"]["rootUri"], uri(&project()));

        lsp.poll();
        assert_eq!(lsp.status(), &Status::Ready);
        assert_eq!(
            server.sent_by("initialized").len(),
            1,
            "the server is told the client is ready for it"
        );
    }

    #[test]
    fn opening_then_editing_a_file_sends_it_once_and_then_the_changes() {
        let (mut lsp, server) = ready();

        lsp.document(&file(), "anchor A:\n");
        let opened = server.sent_by("textDocument/didOpen");
        assert_eq!(opened.len(), 1);
        assert_eq!(opened[0]["params"]["textDocument"]["uri"], uri(&file()));
        assert_eq!(opened[0]["params"]["textDocument"]["text"], "anchor A:\n");

        lsp.document(&file(), "anchor A:\n");
        assert!(
            server.sent_by("textDocument/didChange").is_empty(),
            "text that has not changed is not sent again"
        );

        lsp.document(&file(), "anchor B:\n");
        let changed = server.sent_by("textDocument/didChange");
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0]["params"]["contentChanges"][0]["text"], "anchor B:\n");
        assert_eq!(
            changed[0]["params"]["textDocument"]["version"], 2,
            "each change is a new version of the file"
        );
    }

    #[test]
    fn a_file_opened_before_the_server_answered_is_sent_once_it_has() {
        let (mut lsp, server) = Server::start(&project());
        lsp.document(&file(), "anchor A:\n");

        lsp.poll();

        let opened = server.sent_by("textDocument/didOpen");
        assert_eq!(opened.len(), 1, "the file went out once the server was ready");
        assert_eq!(opened[0]["params"]["textDocument"]["text"], "anchor A:\n");
    }

    #[test]
    fn closing_a_file_tells_the_server_and_forgets_it() {
        let (mut lsp, server) = ready();
        lsp.document(&file(), "anchor A:\n");

        lsp.close(&file());
        assert_eq!(server.sent_by("textDocument/didClose").len(), 1);

        lsp.document(&file(), "anchor A:\n");
        assert_eq!(
            server.sent_by("textDocument/didOpen").len(),
            2,
            "a file that comes back is opened again"
        );
    }

    #[test]
    fn problems_are_kept_per_file_and_replaced_as_they_are_republished() {
        let (mut lsp, server) = ready();
        lsp.document(&file(), "anchor A:\n    key: nope\n");
        assert!(lsp.problems(&file()).is_empty());

        server.publish(
            &file(),
            json!([{
                "range": {
                    "start": { "line": 1, "character": 9 },
                    "end": { "line": 1, "character": 13 },
                },
                "severity": 1,
                "message": "cannot find `nope` in this scope",
            }]),
        );
        lsp.poll();

        let problems = lsp.problems(&file());
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].severity, Severity::Error);
        assert!(problems[0].message.contains("nope"));
        assert_eq!(problems[0].start, Position { line: 1, character: 9 });

        server.publish(&file(), json!([]));
        lsp.poll();
        assert!(
            lsp.problems(&file()).is_empty(),
            "a file that was fixed keeps no old problems"
        );
    }

    #[test]
    fn a_problem_falls_on_the_text_it_is_about() {
        let text = "anchor A:\n    key: nope\n";
        let problem = Problem {
            severity: Severity::Error,
            message: String::new(),
            start: Position { line: 1, character: 9 },
            end: Position { line: 1, character: 13 },
        };

        let range = problem.range_in(text);
        assert_eq!(&text[range], "nope");
    }

    #[test]
    fn an_empty_problem_range_still_covers_something() {
        let text = "anchor A:\n";
        let problem = Problem {
            severity: Severity::Warning,
            message: String::new(),
            start: Position { line: 0, character: 7 },
            end: Position { line: 0, character: 7 },
        };

        let range = problem.range_in(text);
        assert!(!range.is_empty(), "there is something to underline");
    }

    #[test]
    fn hover_is_asked_once_and_answered_in_words() {
        let (mut lsp, server) = ready();
        lsp.document(&file(), "anchor A:\n");
        server.set_hover("```piton\nexport anchor Stack:\n```\n\n**Properties**\n\n- `language`\n");
        let at = Position { line: 0, character: 7 };

        lsp.ask_hover(&file(), at);
        lsp.ask_hover(&file(), at);
        assert_eq!(
            server.sent_by("textDocument/hover").len(),
            1,
            "the same position is only asked about once"
        );

        lsp.poll();
        let hover = lsp.hover(&file(), at).expect("the server answered");
        assert!(hover.contains("export anchor Stack:"), "{hover}");
        assert!(!hover.contains("```"), "the marks are gone: {hover}");
        assert!(!hover.contains('`'), "the marks are gone: {hover}");
        assert!(hover.contains("Properties"));

        assert!(
            lsp.hover(&file(), Position { line: 9, character: 0 }).is_none(),
            "the answer belongs to the position it was asked about"
        );
    }

    #[test]
    fn a_hover_the_server_knows_nothing_about_answers_nothing() {
        let (mut lsp, server) = ready();
        lsp.document(&file(), "anchor A:\n");
        let at = Position { line: 0, character: 0 };

        lsp.ask_hover(&file(), at);
        lsp.poll();

        assert!(lsp.hover(&file(), at).is_none());
        assert!(server.sent_by("textDocument/hover").len() == 1);
    }

    #[test]
    fn the_server_is_answered_when_it_asks_something() {
        let (mut lsp, server) = ready();
        server.say(json!({
            "jsonrpc": "2.0",
            "id": 0,
            "method": "client/registerCapability",
            "params": { "registrations": [] },
        }));

        lsp.poll();

        let answer = server
            .sent()
            .into_iter()
            .find(|message| message["id"] == 0 && message.get("result").is_some())
            .expect("the client answered the server's request");
        assert!(answer["result"].is_null());
    }

    #[test]
    fn a_server_that_stops_is_reported_as_gone() {
        let (mut lsp, server) = ready();
        server.stop();

        lsp.poll();

        assert!(matches!(lsp.status(), Status::Unavailable(_)));
    }

    #[test]
    fn a_missing_server_leaves_a_client_that_says_so() {
        // Nothing is spawned: the client is built the way `start` builds one
        // when the server cannot be run.
        let lsp = Lsp {
            transport: Box::new(Silent),
            root: project(),
            status: Status::Unavailable("piton: not found".into()),
            next_id: 1,
            documents: HashMap::new(),
            problems: HashMap::new(),
            asked: None,
            answer: None,
        };

        assert!(matches!(lsp.status(), Status::Unavailable(_)));
        assert!(lsp.problems(&file()).is_empty());
    }

    #[test]
    fn positions_and_offsets_agree_with_each_other() {
        let text = "anchor A:\n    key: value\nlast";

        assert_eq!(offset_of(text, Position { line: 0, character: 0 }), 0);
        assert_eq!(offset_of(text, Position { line: 1, character: 4 }), 14);
        assert_eq!(&text[offset_of(text, Position { line: 1, character: 9 })..][..5], "value");
        assert_eq!(position_of(text, 14), Position { line: 1, character: 4 });
        assert_eq!(position_of(text, 0), Position::default());

        for offset in 0..=text.len() {
            assert_eq!(
                offset_of(text, position_of(text, offset)),
                offset,
                "the position of {offset} points back at it"
            );
        }
    }

    #[test]
    fn positions_count_the_characters_the_protocol_counts() {
        // The protocol counts UTF-16 units, which is not what Rust indexes by.
        let text = "key: 🌲🌲 here";
        let after_trees = text.find(" here").unwrap();

        let position = position_of(text, after_trees);
        assert_eq!(position.character, 9, "two trees are two units each");
        assert_eq!(offset_of(text, position), after_trees);
    }

    #[test]
    fn a_uri_and_a_path_round_trip_through_each_other() {
        for path in [
            PathBuf::from("/home/someone/overlook/spec/index.pi"),
            PathBuf::from("/tmp/a directory/Thing.pi"),
            PathBuf::from("/tmp/over look/spec/ui/Node Map.pi"),
        ] {
            assert_eq!(path_of(&uri(&path)), Some(path.clone()), "{}", path.display());
        }
        assert!(!uri(Path::new("/a b")).contains(' '), "spaces are escaped");
        assert_eq!(path_of("not a uri"), None);
    }

    /// The real server, over a real pipe.
    ///
    /// Everything else here scripts the server, so the suite needs nothing
    /// installed; this one is the check that the client and the language
    /// actually speak to each other, and is run by hand:
    ///
    /// ```text
    /// cargo test -- --ignored
    /// ```
    #[test]
    #[ignore = "runs the piton language server, which has to be installed"]
    fn the_real_server_answers_the_real_client() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let file = root.join("spec/shape/Stack.pi");
        let mut lsp = Lsp::start(&root);

        /// Poll until `done`, or give up after a few seconds.
        fn until(lsp: &mut Lsp, done: impl Fn(&Lsp) -> bool) -> bool {
            for _ in 0..600 {
                lsp.poll();
                if done(lsp) {
                    return true;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            false
        }

        assert!(
            until(&mut lsp, |lsp| lsp.status() == &Status::Ready),
            "the server started and answered: {:?}",
            lsp.status()
        );

        // A file the compiler cannot resolve has to come back as a problem.
        lsp.document(&file, "export anchor Stack:\n    language: {nope}\n");
        assert!(
            until(&mut lsp, |lsp| !lsp.problems(&file).is_empty()),
            "the server reported the broken reference"
        );
        let problem = &lsp.problems(&file)[0];
        assert_eq!(problem.severity, Severity::Error);
        assert!(problem.message.contains("nope"), "{}", problem.message);

        // And hovering the name it declares says what it is.
        lsp.document(&file, "export anchor Stack:\n    language: Rust\n");
        let at = Position { line: 0, character: 15 };
        lsp.ask_hover(&file, at);
        assert!(
            until(&mut lsp, |lsp| lsp.hover(&file, at).is_some()),
            "the server answered the hover"
        );
        let hover = lsp.hover(&file, at).unwrap();
        assert!(hover.contains("Stack"), "{hover}");
    }

    #[test]
    fn framing_reads_the_messages_a_server_writes() {
        let stream = concat!(
            "Content-Length: 32\r\n\r\n",
            "{\"jsonrpc\":\"2.0\",\"method\":\"one\"}",
            // A header block with more in it than the length.
            "Content-Length: 32\r\nContent-Type: application/vscode-jsonrpc\r\n\r\n",
            "{\"jsonrpc\":\"2.0\",\"method\":\"two\"}",
        );
        let (sender, received) = channel();

        read_messages(BufReader::new(stream.as_bytes()), &sender);

        let messages: Vec<Value> = received.try_iter().collect();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["method"], "one");
        assert_eq!(messages[1]["method"], "two");
    }

    #[test]
    fn framing_writes_what_a_server_can_read_back() {
        let (sender, messages) = channel();
        sender.send(json!({ "jsonrpc": "2.0", "method": "one" })).unwrap();
        drop(sender);
        let mut written = Vec::new();

        write_messages(&mut written, messages, &AtomicBool::new(true));

        let written = String::from_utf8(written).unwrap();
        let (header, body) = written.split_once("\r\n\r\n").expect("a header and a body");
        assert_eq!(header, format!("Content-Length: {}", body.len()));
        assert_eq!(
            serde_json::from_str::<Value>(body).unwrap()["method"],
            "one"
        );
    }
}
