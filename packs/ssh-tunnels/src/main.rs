//! SSH tunnels: an sshremote extension (API 2). It runs `ssh -N` for port forwards: those of the
//! hosts in ~/.ssh/config, and those made in its own panel, which it keeps (and can start with the
//! app). The status bar shows how many run. It talks with sshremote over stdin and stdout, one
//! JSON-RPC message per line.

use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

/// A host of ~/.ssh/config with forwards, and what they are.
struct Host {
    name: String,
    forwards: Vec<String>,
}

/// A tunnel made in the panel and kept: through `host`, of `kind` ("local", "remote" or "socks").
#[derive(Clone, PartialEq, Debug)]
struct Saved {
    host: String,
    kind: String,
    port: u16,
    /// HOST:PORT it leads to; empty for SOCKS.
    dest: String,
    /// Started when the app starts.
    auto: bool,
}

impl Saved {
    /// Its name in the panel and as the key of its running ssh.
    fn key(&self) -> String {
        match self.kind.as_str() {
            "remote" => format!("{}: host {} > here {}", self.host, self.port, self.dest),
            "socks" => format!("{}: SOCKS on {}", self.host, self.port),
            _ => format!("{}: {} > {}", self.host, self.port, self.dest),
        }
    }

    /// The ssh option that makes it.
    fn args(&self) -> Vec<String> {
        match self.kind.as_str() {
            "remote" => vec!["-R".into(), format!("{}:{}", self.port, self.dest)],
            "socks" => vec!["-D".into(), self.port.to_string()],
            _ => vec!["-L".into(), format!("{}:{}", self.port, self.dest)],
        }
    }

    fn to_json(&self) -> Value {
        json!({ "host": self.host, "kind": self.kind, "port": self.port, "dest": self.dest, "auto": self.auto })
    }

    fn from_json(v: &Value) -> Option<Saved> {
        let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
        let t = Saved { host: s("host"), kind: s("kind"), port: v.get("port")?.as_u64()?.try_into().ok()?, dest: s("dest"), auto: v.get("auto").and_then(|x| x.as_bool()).unwrap_or(false) };
        t.valid().then_some(t)
    }

    /// Nothing ssh would read as an option, a port that exists, and somewhere to lead to.
    fn valid(&self) -> bool {
        let safe = |s: &str| !s.starts_with('-') && !s.chars().any(char::is_whitespace);
        !self.host.is_empty()
            && safe(&self.host)
            && self.port > 0
            && matches!(self.kind.as_str(), "local" | "remote" | "socks")
            && (self.kind == "socks" || (self.dest.contains(':') && safe(&self.dest)))
    }
}

enum Msg {
    App(Value),
    Tick,
    Closed,
}

fn main() {
    let (tx, rx) = mpsc::channel();
    let input = tx.clone();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            if let Ok(v) = serde_json::from_str(&line) {
                let _ = input.send(Msg::App(v));
            }
        }
        let _ = input.send(Msg::Closed);
    });
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_secs(2));
            if tx.send(Msg::Tick).is_err() {
                break;
            }
        }
    });
    let mut saved = load();
    let mut running: BTreeMap<String, Child> = BTreeMap::new();
    let mut errors: BTreeMap<String, String> = BTreeMap::new();
    let mut panel_open = false;
    let mut shown = String::new();
    // The kept tunnels that start with the app.
    for t in saved.iter().filter(|t| t.auto) {
        match start(&t.host, &t.args()) {
            Ok(c) => {
                running.insert(t.key(), c);
            }
            Err(e) => {
                errors.insert(t.key(), e);
            }
        }
    }
    for msg in rx {
        let mut changed = false;
        match msg {
            Msg::Closed => break,
            Msg::Tick => {
                // A tunnel whose ssh ended: why, if it said.
                let ended: Vec<String> = running.iter_mut().filter_map(|(h, c)| c.try_wait().ok().flatten().map(|_| h.clone())).collect();
                for h in ended {
                    let mut c = running.remove(&h).expect("listed");
                    let mut why = String::new();
                    if let Some(mut e) = c.stderr.take() {
                        let _ = std::io::Read::read_to_string(&mut e, &mut why);
                    }
                    let why = plain(why.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("ssh ended").trim());
                    notify(&format!("The tunnel {h} stopped: {why}"));
                    errors.insert(h, why);
                    changed = true;
                }
            }
            Msg::App(v) => {
                let method = v.get("method").and_then(|m| m.as_str()).unwrap_or("");
                let p = v.get("params").cloned().unwrap_or(Value::Null);
                match method {
                    "initialize" => changed = true,
                    "shutdown" => break,
                    "command" => match p.get("id").and_then(|i| i.as_str()) {
                        Some("show") => {
                            panel_open = true;
                            shown.clear();
                            changed = true;
                        }
                        Some("forward") => ask_forward(),
                        Some("stop_all") => {
                            for (_, mut c) in std::mem::take(&mut running) {
                                let _ = c.kill();
                                let _ = c.wait();
                            }
                            changed = true;
                        }
                        _ => {}
                    },
                    "action" => {
                        let key = p.pointer("/args/host").and_then(|h| h.as_str()).unwrap_or("").to_string();
                        let known = hosts().iter().any(|h| h.name == key);
                        let kept = saved.iter().find(|t| t.key() == key).cloned();
                        match p.get("action").and_then(|a| a.as_str()) {
                            Some("start") if (known || kept.is_some()) && !running.contains_key(&key) => {
                                errors.remove(&key);
                                let args = kept.as_ref().map(Saved::args).unwrap_or_default();
                                let host = kept.as_ref().map_or(key.clone(), |t| t.host.clone());
                                match start(&host, &args) {
                                    Ok(c) => {
                                        running.insert(key, c);
                                    }
                                    Err(e) => {
                                        errors.insert(key, e);
                                    }
                                }
                                changed = true;
                            }
                            Some("stop") => {
                                if let Some(mut c) = running.remove(&key) {
                                    let _ = c.kill();
                                    let _ = c.wait();
                                }
                                changed = true;
                            }
                            Some("forget") => {
                                if let Some(mut c) = running.remove(&key) {
                                    let _ = c.kill();
                                    let _ = c.wait();
                                }
                                saved.retain(|t| t.key() != key);
                                errors.remove(&key);
                                keep(&saved);
                                changed = true;
                            }
                            Some("auto") => {
                                if let Some(t) = saved.iter_mut().find(|t| t.key() == key) {
                                    t.auto = !t.auto;
                                    keep(&saved);
                                    changed = true;
                                }
                            }
                            Some("shell") if known => send("terminal.open", json!({ "argv": ["ssh", key], "label": key }), Some(1)),
                            Some("forward") => ask_forward(),
                            Some("stop_all") => {
                                for (_, mut c) in std::mem::take(&mut running) {
                                    let _ = c.kill();
                                    let _ = c.wait();
                                }
                                changed = true;
                            }
                            Some("refresh") => {
                                shown.clear();
                                changed = true;
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
                // The forward form answered: kept, and started.
                if v.get("id").and_then(|i| i.as_u64()) == Some(FORM)
                    && let Some(vals) = v.pointer("/result/values")
                {
                    let s = |k: &str| vals.get(k).and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
                    let kind = match s("kind").as_str() {
                        k if k.starts_with("Remote") => "remote",
                        k if k.starts_with("SOCKS") => "socks",
                        _ => "local",
                    };
                    let port = vals.get("port").and_then(|l| l.as_f64()).unwrap_or(0.0);
                    let t = Saved {
                        host: s("host"),
                        kind: kind.into(),
                        port: if (1.0..65536.0).contains(&port) { port as u16 } else { 0 },
                        dest: if kind == "socks" { String::new() } else { s("dest") },
                        auto: vals.get("auto").and_then(|a| a.as_bool()).unwrap_or(false),
                    };
                    if t.valid() {
                        let key = t.key();
                        if !saved.iter().any(|o| o.key() == key) {
                            saved.push(t.clone());
                            keep(&saved);
                        }
                        if !running.contains_key(&key) {
                            errors.remove(&key);
                            match start(&t.host, &t.args()) {
                                Ok(c) => {
                                    running.insert(key, c);
                                }
                                Err(e) => {
                                    errors.insert(key, e);
                                }
                            }
                        }
                        panel_open = true;
                        changed = true;
                    } else {
                        notify("A host, a port from 1 to 65535 and, unless it is SOCKS, HOST:PORT to lead to are needed");
                    }
                }
                // A request of ours answered with an error (a cancelled form among them): say so.
                if let Some(e) = v.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str())
                    && e != "cancelled"
                {
                    notify(e);
                }
            }
        }
        if changed {
            let all = hosts();
            let total = all.len() + saved.len();
            let on = running.len();
            // Nothing to say while there are no tunnels: the bar stays free for the app.
            if total == 0 && on == 0 {
                send("status.remove", json!({ "id": "count" }), None);
            } else {
                let text = format!("⇄ {on}/{} tunnels", total.max(on));
                send("status.set", json!({ "id": "count", "text": text, "tooltip": "SSH tunnels: click to show them", "command": "show" }), None);
            }
            if panel_open {
                let md = panel(&all, &saved, &running, &errors);
                if md != shown {
                    send("panel.show", json!({ "id": "tunnels", "title": "SSH tunnels", "markdown": md }), None);
                    shown = md;
                }
            }
        }
    }
    for (_, mut c) in running {
        let _ = c.kill();
    }
}

fn send(method: &str, params: Value, id: Option<u64>) {
    let mut msg = json!({ "jsonrpc": "2.0", "method": method, "params": params });
    if let Some(id) = id {
        msg["id"] = json!(id);
    }
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{msg}");
    let _ = out.flush();
}

fn notify(text: &str) {
    send("notify", json!({ "text": text }), None);
}

/// What ssh said, in the words of someone who only wanted a port forwarded.
fn plain(why: &str) -> String {
    let l = why.to_lowercase();
    if l.contains("permission denied") {
        "the host wants a password, which a tunnel cannot type: give it your key (Tools > SSH keys > Copy to a host)".into()
    } else if l.contains("address already in use") || l.contains("cannot listen") || l.contains("forwarding failed") {
        "that port is already taken; pick another".into()
    } else if l.contains("could not resolve") || l.contains("no such host") {
        "the host's name is not known".into()
    } else if l.contains("timed out") || l.contains("connection refused") || l.contains("unreachable") {
        "the host could not be reached".into()
    } else {
        why.to_string()
    }
}

/// `ssh -N HOST` with `extra` forwards (and those of ~/.ssh/config); it fails at once if one
/// cannot be made, and never waits on a password prompt nobody sees.
fn start(host: &str, extra: &[String]) -> Result<Child, String> {
    let mut cmd = Command::new("ssh");
    cmd.args(["-N", "-o", "ExitOnForwardFailure=yes", "-o", "BatchMode=yes", "-o", "ServerAliveInterval=30"]).args(extra).arg(host);
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd.spawn().map_err(|e| format!("ssh: {e}"))
}

fn home() -> std::path::PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(Into::into).unwrap_or_default()
}

/// Where the kept tunnels are: beside sshremote's other settings (the extension's own folder is
/// replaced by an update), or the home folder when run on its own.
fn store() -> std::path::PathBuf {
    let base = std::env::var_os("SSHREMOTE_EXTENSION_DIR")
        .map(std::path::PathBuf::from)
        .and_then(|d| d.parent().and_then(|p| p.parent()).map(|p| p.join("extension-data")))
        .unwrap_or_else(|| home().join(".sshremote-extension-data"));
    base.join("ssh-tunnels.json")
}

fn load() -> Vec<Saved> {
    let text = std::fs::read_to_string(store()).unwrap_or_default();
    let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    v.as_array().map(|a| a.iter().filter_map(Saved::from_json).collect()).unwrap_or_default()
}

fn keep(saved: &[Saved]) {
    let f = store();
    if let Some(d) = f.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let text = Value::Array(saved.iter().map(Saved::to_json).collect()).to_string();
    if let Err(e) = std::fs::write(&f, text) {
        notify(&format!("The tunnels could not be kept: {e}"));
    }
}

/// Asks for a tunnel, with a form; the answer comes back with the id FORM.
fn ask_forward() {
    let names = host_names();
    let host = if names.is_empty() {
        json!({ "id": "host", "label": "Host", "type": "text" })
    } else {
        json!({ "id": "host", "label": "Host", "type": "choice", "choices": names })
    };
    let fields = json!([
        host,
        { "id": "kind", "label": "Kind", "type": "choice", "choices": [
            "Local: a port here leads to HOST:PORT seen from the host",
            "Remote: a port on the host leads to HOST:PORT seen from here",
            "SOCKS: a proxy here that goes out through the host"
        ] },
        { "id": "port", "label": "Port", "type": "number", "value": 8080 },
        { "id": "dest", "label": "Leads to (not for SOCKS)", "type": "text", "value": "localhost:80" },
        { "id": "auto", "label": "Start it with the app", "type": "bool", "value": false }
    ]);
    send("input.form", json!({ "title": "New tunnel", "prompt": "Kept in the panel, to start and stop again. The host needs your key: a tunnel cannot type a password.", "submit": "Start", "fields": fields }), Some(FORM));
}

/// The id of the forward form's request, to know its answer.
const FORM: u64 = 100;

/// Every host named in ~/.ssh/config, for the form.
fn host_names() -> Vec<String> {
    let text = std::fs::read_to_string(home().join(".ssh").join("config")).unwrap_or_default();
    let mut out: Vec<String> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let Some((k, v)) = line.split_once(char::is_whitespace) else { continue };
        if k.eq_ignore_ascii_case("host") {
            for n in v.split_whitespace().filter(|n| !n.contains(['*', '?', '!'])) {
                if !out.iter().any(|o| o == n) {
                    out.push(n.to_string());
                }
            }
        }
    }
    out
}

/// The hosts of ~/.ssh/config that have forwards.
fn hosts() -> Vec<Host> {
    let text = std::fs::read_to_string(home().join(".ssh").join("config")).unwrap_or_default();
    parse(&text)
}

fn parse(text: &str) -> Vec<Host> {
    let mut out: Vec<Host> = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let (key, val) = line.split_once(|c: char| c.is_whitespace() || c == '=').unwrap_or((line, ""));
        let val = val.trim().trim_start_matches('=').trim();
        match key.to_ascii_lowercase().as_str() {
            "host" => {
                current.clear();
                for name in val.split_whitespace().filter(|n| !n.contains(['*', '?', '!'])) {
                    let i = out.iter().position(|h| h.name == name).unwrap_or_else(|| {
                        out.push(Host { name: name.to_string(), forwards: Vec::new() });
                        out.len() - 1
                    });
                    current.push(i);
                }
            }
            "match" => current.clear(),
            k @ ("localforward" | "remoteforward" | "dynamicforward") => {
                let kind = match k {
                    "localforward" => "local",
                    "remoteforward" => "remote",
                    _ => "socks",
                };
                for &i in &current {
                    out[i].forwards.push(format!("{kind} {val}"));
                }
            }
            _ => {}
        }
    }
    out.retain(|h| !h.forwards.is_empty());
    out
}

fn state(key: &str, running: &BTreeMap<String, Child>, errors: &BTreeMap<String, String>) -> String {
    if running.contains_key(key) {
        "running".into()
    } else {
        errors.get(key).map_or("stopped".to_string(), |e| format!("stopped: {}", e.replace('|', "/")))
    }
}

fn panel(all: &[Host], saved: &[Saved], running: &BTreeMap<String, Child>, errors: &BTreeMap<String, String>) -> String {
    let mut md = String::from("A tunnel carries a port through an SSH host: a database or a web page that only the host can reach opens here, on localhost.\n\n[New tunnel](sshremote:forward) · [Stop all](sshremote:stop_all) · [Look again](sshremote:refresh)\n\n");
    if !saved.is_empty() {
        md.push_str("**Yours**\n\n| Tunnel | State | |\n|---|---|---|\n");
        for t in saved {
            let k = t.key();
            let q = escape(&k);
            let act = if running.contains_key(&k) { format!("[Stop](sshremote:stop?host={q})") } else { format!("[Start](sshremote:start?host={q})") };
            let auto = if t.auto { "starts with the app" } else { "start with the app" };
            md.push_str(&format!("| {k} | {} | {act} · [{auto}](sshremote:auto?host={q}) · [Forget](sshremote:forget?host={q}) |\n", state(&k, running, errors)));
        }
        md.push('\n');
    }
    if !all.is_empty() {
        md.push_str("**From ~/.ssh/config**\n\n| Host | Forwards | State | |\n|---|---|---|---|\n");
        for h in all {
            let q = escape(&h.name);
            let act = if running.contains_key(&h.name) { format!("[Stop](sshremote:stop?host={q})") } else { format!("[Start](sshremote:start?host={q})") };
            md.push_str(&format!("| {} | {} | {} | {act} · [Shell](sshremote:shell?host={q}) |\n", h.name, h.forwards.join(", "), state(&h.name, running, errors)));
        }
        md.push('\n');
    }
    if saved.is_empty() && all.is_empty() {
        md.push_str("No tunnels yet. Make one with **New tunnel**, or add `LocalForward` lines to a host in ~/.ssh/config, for example:\n\n```\nHost db\n  HostName db.example.com\n  LocalForward 5432 localhost:5432\n```\n");
    }
    md
}

/// A tunnel's name as a link argument.
fn escape(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forwards_are_read_per_host() {
        let hosts = parse("Host db web\n  LocalForward 5432 localhost:5432\nHost *\n  DynamicForward 1080\nHost plain\n  User x\nHost web\n  RemoteForward 9000 localhost:9000\n");
        let names: Vec<_> = hosts.iter().map(|h| (h.name.as_str(), h.forwards.len())).collect();
        assert_eq!(names, [("db", 1), ("web", 2)]);
    }

    #[test]
    fn kept_tunnels_round_trip_and_refuse_options() {
        let t = Saved { host: "db".into(), kind: "local".into(), port: 5432, dest: "localhost:5432".into(), auto: true };
        assert_eq!(Saved::from_json(&t.to_json()), Some(t.clone()));
        assert_eq!(t.args(), ["-L", "5432:localhost:5432"]);
        let socks = Saved { kind: "socks".into(), dest: String::new(), port: 1080, ..t.clone() };
        assert!(socks.valid() && socks.args() == ["-D", "1080"]);
        assert!(!Saved { host: "-oProxyCommand=x".into(), ..t.clone() }.valid());
        assert!(!Saved { dest: "-x:1".into(), ..t.clone() }.valid());
        assert!(!Saved { port: 0, ..t }.valid());
    }

    #[test]
    fn ssh_errors_in_plain_words() {
        assert!(plain("user@db: Permission denied (publickey,password).").contains("key"));
        assert!(plain("bind [127.0.0.1]:8080: Address already in use").contains("taken"));
    }
}
