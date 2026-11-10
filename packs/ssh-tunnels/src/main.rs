//! SSH tunnels: an sshremote extension (API 2). It reads the port forwards of the hosts in
//! ~/.ssh/config, runs `ssh -N HOST` for the ones started from its panel, and keeps the status bar
//! and the panel up to date. It talks with sshremote over stdin and stdout, one JSON-RPC message
//! per line.

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
    let mut running: BTreeMap<String, Child> = BTreeMap::new();
    let mut errors: BTreeMap<String, String> = BTreeMap::new();
    let mut panel_open = false;
    let mut shown = String::new();
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
                    let why = why.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("ssh ended").trim().to_string();
                    notify(&format!("The tunnels of {h} stopped: {why}"));
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
                            }
                            changed = true;
                        }
                        _ => {}
                    },
                    "action" => {
                        let host = p.pointer("/args/host").and_then(|h| h.as_str()).unwrap_or("").to_string();
                        let known = hosts().iter().any(|h| h.name == host);
                        match p.get("action").and_then(|a| a.as_str()) {
                            Some("start") if known && !running.contains_key(&host) => {
                                errors.remove(&host);
                                match start(&host, &[]) {
                                    Ok(c) => {
                                        running.insert(host, c);
                                    }
                                    Err(e) => {
                                        errors.insert(host, e);
                                    }
                                }
                                changed = true;
                            }
                            Some("stop") => {
                                if let Some(mut c) = running.remove(&host) {
                                    let _ = c.kill();
                                    let _ = c.wait();
                                }
                                changed = true;
                            }
                            Some("shell") if known => send("terminal.open", json!({ "argv": ["ssh", host], "label": host }), Some(1)),
                            Some("forward") => {
                                ask_forward();
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
                // The forward form answered.
                if v.get("id").and_then(|i| i.as_u64()) == Some(FORM)
                    && let Some(vals) = v.pointer("/result/values")
                {
                    let host = vals.get("host").and_then(|h| h.as_str()).unwrap_or("").trim().to_string();
                    let local = vals.get("local").and_then(|l| l.as_f64()).unwrap_or(0.0) as u32;
                    let dest = vals.get("dest").and_then(|d| d.as_str()).unwrap_or("").trim().to_string();
                    let ok = !host.is_empty() && !host.starts_with('-') && (1..65536).contains(&local) && dest.contains(':') && !dest.starts_with('-');
                    if ok {
                        let key = format!("{host}: {local} > {dest}");
                        match start(&host, &["-L".into(), format!("{local}:{dest}")]) {
                            Ok(c) => {
                                running.insert(key, c);
                            }
                            Err(e) => notify(&e),
                        }
                        panel_open = true;
                        changed = true;
                    } else {
                        notify("A host, a port from 1 to 65535 and HOST:PORT to lead to are needed");
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
            let on = running.len();
            let text = if all.is_empty() { "⇄ no tunnels".to_string() } else { format!("⇄ {on}/{} tunnels", all.len()) };
            send("status.set", json!({ "id": "count", "text": text, "tooltip": "SSH tunnels from ~/.ssh/config: click to show them", "command": "show" }), None);
            if panel_open {
                let md = panel(&all, &running, &errors);
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

/// `ssh -N HOST`: the forwards come from ~/.ssh/config; it fails at once if one cannot be made,
/// and never waits on a password prompt nobody sees.
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

/// The hosts of ~/.ssh/config (and the files it includes there) that have forwards.
/// Asks which port to forward, with a form; the answer comes back with the id FORM.
fn ask_forward() {
    let names = host_names();
    let host = if names.is_empty() {
        json!({ "id": "host", "label": "Host", "type": "text" })
    } else {
        json!({ "id": "host", "label": "Host", "type": "choice", "choices": names })
    };
    let fields = json!([
        host,
        { "id": "local", "label": "Port here", "type": "number", "value": 8080 },
        { "id": "dest", "label": "Leads to (from the host)", "type": "text", "value": "localhost:80" }
    ]);
    send("input.form", json!({ "title": "Forward a port", "prompt": "Opens a port on this computer that leads through the host.", "submit": "Forward", "fields": fields }), Some(FORM));
}

/// The id of the forward form's request, to know its answer.
const FORM: u64 = 100;

/// Every host named in ~/.ssh/config, for the forward form.
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

fn panel(all: &[Host], running: &BTreeMap<String, Child>, errors: &BTreeMap<String, String>) -> String {
    let mut md = String::from("Tunnels are the LocalForward, RemoteForward and DynamicForward lines of your ~/.ssh/config hosts. Start runs `ssh -N` for the host; it needs a key, as nothing can ask for a password here.\n\n");
    // Forwards started from the form, whatever ~/.ssh/config holds.
    let extra: Vec<&String> = running.keys().filter(|k| !all.iter().any(|h| &h.name == *k)).collect();
    if !extra.is_empty() {
        md.push_str("Started here:\n\n| Forward | |\n|---|---|\n");
        for k in extra {
            md.push_str(&format!("| {k} | [Stop](sshremote:stop?host={}) |\n", escape(k)));
        }
        md.push('\n');
    }
    md.push_str("[Forward a port](sshremote:forward)\n\n");
    if all.is_empty() {
        md.push_str("No host has a forward in ~/.ssh/config yet. Add one there to keep it, for example:\n\n```\nHost db\n  HostName db.example.com\n  LocalForward 5432 localhost:5432\n```\n\n[Look again](sshremote:refresh)\n");
        return md;
    }
    md.push_str("| Host | Forwards | State | |\n|---|---|---|---|\n");
    for h in all {
        let on = running.contains_key(&h.name);
        let q = h.name.replace(' ', "%20");
        let state = if on {
            "running".to_string()
        } else {
            errors.get(&h.name).map_or("stopped".to_string(), |e| format!("stopped: {}", e.replace('|', "/")))
        };
        let act = if on { format!("[Stop](sshremote:stop?host={q})") } else { format!("[Start](sshremote:start?host={q})") };
        md.push_str(&format!("| {} | {} | {state} | {act} · [Shell](sshremote:shell?host={q}) |\n", h.name, h.forwards.join(", ")));
    }
    md.push_str("\n[Look again](sshremote:refresh)\n");
    md
}

/// A tunnel's name as a link argument.
fn escape(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn forwards_are_read_per_host() {
        let hosts = super::parse("Host db web\n  LocalForward 5432 localhost:5432\nHost *\n  DynamicForward 1080\nHost plain\n  User x\nHost web\n  RemoteForward 9000 localhost:9000\n");
        let names: Vec<_> = hosts.iter().map(|h| (h.name.as_str(), h.forwards.len())).collect();
        assert_eq!(names, [("db", 1), ("web", 2)]);
    }
}
