//! Text tools for the terminal's selection: each command takes the selected text and gives back
//! a new text, typed into the terminal as a paste.

use std::alloc::{Layout, alloc as raw_alloc};

static mut OUT: Vec<u8> = Vec::new();

#[unsafe(no_mangle)]
pub extern "C" fn alloc(len: i32) -> i32 {
    let len = (len as usize).max(1);
    unsafe { raw_alloc(Layout::from_size_align(len, 1).unwrap()) as i32 }
}

/// The selection out of the input: {"selection": "...", "host": "..."}.
fn selection(ptr: i32, len: i32) -> String {
    let input = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    let text = String::from_utf8_lossy(input);
    let Some(start) = text.find("\"selection\":\"") else { return String::new() };
    let mut out = String::new();
    let mut chars = text[start + 13..].chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => break,
            '\\' => match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('u') => {
                    let hex: String = chars.by_ref().take(4).collect();
                    if let Some(c) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                        out.push(c);
                    }
                }
                Some(c) => out.push(c),
                None => break,
            },
            c => out.push(c),
        }
    }
    out
}

fn give(text: String) -> i64 {
    unsafe {
        let out = &mut *std::ptr::addr_of_mut!(OUT);
        *out = text.into_bytes();
        ((out.as_ptr() as i64) << 32) | out.len() as i64
    }
}

/// Upper case.
#[unsafe(no_mangle)]
pub extern "C" fn run_upper(ptr: i32, len: i32) -> i64 {
    give(selection(ptr, len).to_uppercase())
}

/// Lower case.
#[unsafe(no_mangle)]
pub extern "C" fn run_lower(ptr: i32, len: i32) -> i64 {
    give(selection(ptr, len).to_lowercase())
}

/// The selection quoted for a POSIX shell, as one word.
#[unsafe(no_mangle)]
pub extern "C" fn run_quote(ptr: i32, len: i32) -> i64 {
    give(format!("'{}'", selection(ptr, len).replace('\'', "'\\''")))
}

/// Lines joined into one, separated by spaces.
#[unsafe(no_mangle)]
pub extern "C" fn run_join(ptr: i32, len: i32) -> i64 {
    give(selection(ptr, len).split_whitespace().collect::<Vec<_>>().join(" "))
}

/// Base64 of the selection.
#[unsafe(no_mangle)]
pub extern "C" fn run_base64(ptr: i32, len: i32) -> i64 {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let b = selection(ptr, len).into_bytes();
    let mut out = String::new();
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            out.push(if i <= c.len() { T[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    give(out)
}
