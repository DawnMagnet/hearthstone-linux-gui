//! Wire format between the bridge and `commerce_helper.exe` (little endian).
//! Must match `helper/commerce_helper.c`.
//!
//! Bridge -> helper:
//! - call: `1, library u8, name_len u16, name, ret u8, argc u8, args...`;
//!   each arg is `kind u8` then `u64` (int), `present u8, len u32, bytes`
//!   (string) or nothing (listener, log hook).
//! - event done: `2`.
//!
//! Helper -> bridge:
//! - result: `1, u64`, then for string results `present u8, len u32, bytes`.
//! - event: `2, owner u64, event u64` (the listener must run, then reply
//!   with "event done"; calls made meanwhile are served first).

use std::io::{self, Read, Write};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgKind {
    Int,
    Str,
    Listener,
    Log,
    StringCallback,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetKind {
    Int,
    Str,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Arg {
    Int(u64),
    Str(Option<Vec<u8>>),
    Listener,
    Log,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Reply {
    Int(u64),
    Str(Option<Vec<u8>>),
}

#[derive(Debug, PartialEq, Eq)]
pub enum Message {
    Result(Reply),
    Event { owner: u64, event: u64 },
}

const CALL: u8 = 1;
const EVENT_DONE: u8 = 2;
const RESULT: u8 = 1;
const EVENT: u8 = 2;

pub fn write_call(
    out: &mut impl Write,
    library: u8,
    name: &str,
    args: &[Arg],
    ret: RetKind,
) -> io::Result<()> {
    let mut message = vec![CALL, library];
    message.extend((name.len() as u16).to_le_bytes());
    message.extend(name.as_bytes());
    message.push(match ret {
        RetKind::Int => 0,
        RetKind::Str => 1,
    });
    message.push(args.len() as u8);
    for arg in args {
        match arg {
            Arg::Int(value) => {
                message.push(0);
                message.extend(value.to_le_bytes());
            }
            Arg::Str(text) => {
                message.push(1);
                write_string(&mut message, text.as_deref());
            }
            Arg::Listener => message.push(2),
            Arg::Log => message.push(3),
        }
    }
    out.write_all(&message)?;
    out.flush()
}

pub fn write_event_done(out: &mut impl Write) -> io::Result<()> {
    out.write_all(&[EVENT_DONE])?;
    out.flush()
}

pub fn read_message(input: &mut impl Read, ret: RetKind) -> io::Result<Message> {
    match read_array::<1>(input)?[0] {
        RESULT => {
            let value = u64::from_le_bytes(read_array(input)?);
            Ok(Message::Result(match ret {
                RetKind::Int => Reply::Int(value),
                RetKind::Str => Reply::Str(read_string(input)?),
            }))
        }
        EVENT => Ok(Message::Event {
            owner: u64::from_le_bytes(read_array(input)?),
            event: u64::from_le_bytes(read_array(input)?),
        }),
        tag => Err(io::Error::other(format!(
            "unexpected message {tag} from helper"
        ))),
    }
}

fn write_string(message: &mut Vec<u8>, text: Option<&[u8]>) {
    match text {
        Some(text) => {
            message.push(1);
            message.extend((text.len() as u32).to_le_bytes());
            message.extend(text);
        }
        None => message.push(0),
    }
}

fn read_string(input: &mut impl Read) -> io::Result<Option<Vec<u8>>> {
    if read_array::<1>(input)?[0] == 0 {
        return Ok(None);
    }
    let len = u32::from_le_bytes(read_array(input)?) as usize;
    let mut text = vec![0; len];
    input.read_exact(&mut text)?;
    Ok(Some(text))
}

fn read_array<const N: usize>(input: &mut impl Read) -> io::Result<[u8; N]> {
    let mut bytes = [0; N];
    input.read_exact(&mut bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_a_call() {
        let mut out = Vec::new();
        let args = [
            Arg::Int(7),
            Arg::Str(Some(b"WTCG".to_vec())),
            Arg::Str(None),
            Arg::Listener,
        ];
        write_call(&mut out, 0, "f", &args, RetKind::Str).unwrap();
        let mut expected = vec![1, 0, 1, 0, b'f', 1, 4];
        expected.extend([0, 7, 0, 0, 0, 0, 0, 0, 0]);
        expected.extend([1, 1, 4, 0, 0, 0, b'W', b'T', b'C', b'G']);
        expected.extend([1, 0]);
        expected.push(2);
        assert_eq!(out, expected);
    }

    #[test]
    fn decodes_results_and_events() {
        let mut input: &[u8] = &[
            1, 42, 0, 0, 0, 0, 0, 0, 0, // int result
            1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 0, 0, 0, b'o', b'k', // string result
            2, 1, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, // event
        ];
        assert_eq!(
            read_message(&mut input, RetKind::Int).unwrap(),
            Message::Result(Reply::Int(42))
        );
        assert_eq!(
            read_message(&mut input, RetKind::Str).unwrap(),
            Message::Result(Reply::Str(Some(b"ok".to_vec())))
        );
        assert_eq!(
            read_message(&mut input, RetKind::Int).unwrap(),
            Message::Event { owner: 1, event: 2 }
        );
        assert!(read_message(&mut input, RetKind::Int).is_err());
    }
}
