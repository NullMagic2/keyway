//! Module: keyway_core::protocol
//! Purpose: Messages between the router and the agent, one JSON object per line.
//! Created: 2026-11-08
//! Architecture: Both binaries come from one build, so a version mismatch is an error.
//! Messages carry shortcuts and finished Alt codes, never ordinary keystrokes.

use crate::alt_code::{AltCodeEntry, AltCodeRules};
use crate::hotkey::{ShortcutAction, ShortcutBinding};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fmt;

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024;
pub const DEFAULT_SOCKET_PATH: &str = "/run/keyway/router.sock";
const MESSAGE_SEPARATOR: u8 = b'\n';

/// What the router needs to know to interpret keys.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouterConfig {
    pub shortcuts: Vec<ShortcutBinding>,
    pub alt_codes: AltCodeRules,
    pub excluded_devices: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum RouterMessage {
    Hello { version: u32 },
    Shortcut(ShortcutAction),
    AltCode(AltCodeEntry),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentMessage {
    Hello { version: u32 },
    Configure(RouterConfig),
    SetAllowRightAlt(bool),
}

#[derive(Debug)]
pub enum ProtocolError {
    TooLong(usize),
    NotText,
    Malformed(serde_json::Error),
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLong(length) => {
                write!(
                    formatter,
                    "message of {length} bytes exceeds {MAX_MESSAGE_BYTES}"
                )
            }
            Self::NotText => formatter.write_str("message is not valid UTF-8"),
            Self::Malformed(error) => write!(formatter, "malformed message: {error}"),
        }
    }
}

impl std::error::Error for ProtocolError {}

/// Encodes a message as one line, including its newline.
pub fn encode_message<T: Serialize>(message: &T) -> Result<String, ProtocolError> {
    let mut line = serde_json::to_string(message).map_err(ProtocolError::Malformed)?;

    if line.len() > MAX_MESSAGE_BYTES {
        return Err(ProtocolError::TooLong(line.len()));
    }

    line.push(char::from(MESSAGE_SEPARATOR));

    Ok(line)
}

/// Decodes one received line, checking its size before parsing it.
pub fn decode_message<T: DeserializeOwned>(line: &str) -> Result<T, ProtocolError> {
    if line.len() > MAX_MESSAGE_BYTES {
        return Err(ProtocolError::TooLong(line.len()));
    }

    serde_json::from_str(line).map_err(ProtocolError::Malformed)
}

/// Splits a byte stream into lines, however the reads happen to divide it.
#[derive(Clone, Debug, Default)]
pub struct LineFramer {
    buffer: Vec<u8>,
}

impl LineFramer {
    /// Adds received bytes and returns the lines they complete, without their newlines.
    /// Any error means the sender must be dropped, as the stream can no longer be trusted.
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<String>, ProtocolError> {
        self.buffer.extend_from_slice(bytes);

        let mut lines = Vec::new();

        while let Some(end) = self.buffer.iter().position(|b| *b == MESSAGE_SEPARATOR) {
            let line: Vec<u8> = self.buffer.drain(..=end).collect();

            match String::from_utf8(line) {
                Ok(text) => lines.push(text.trim_end().to_string()),
                Err(_) => return Err(ProtocolError::NotText),
            }
        }

        if self.buffer.len() > MAX_MESSAGE_BYTES {
            return Err(ProtocolError::TooLong(self.buffer.len()));
        }

        Ok(lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alt_code::{AltCodeRadix, NumLockPolicy};
    use crate::hotkey::Chord;
    use crate::layout_id::LayoutId;

    #[test]
    fn router_messages_survive_a_round_trip() {
        let layout = LayoutId::try_from("gr+simple".to_string()).unwrap();
        let entry = AltCodeEntry::new(AltCodeRadix::Decimal, "0128".to_string()).unwrap();
        let messages = [
            RouterMessage::Hello {
                version: PROTOCOL_VERSION,
            },
            RouterMessage::Shortcut(ShortcutAction::SelectLayout(layout)),
            RouterMessage::AltCode(entry),
        ];

        for message in messages {
            let line = encode_message(&message).unwrap();

            assert_eq!(line.matches('\n').count(), 1);
            assert!(line.ends_with('\n'));
            assert_eq!(decode_message::<RouterMessage>(&line).unwrap(), message);
        }
    }

    #[test]
    fn agent_messages_survive_a_round_trip() {
        let binding = ShortcutBinding {
            chord: Chord::ALT_SHIFT,
            action: ShortcutAction::NextLayout,
        };
        let alt_codes = AltCodeRules {
            enabled: true,
            numlock: NumLockPolicy::Ignored,
            hex_numpad: false,
            allow_right_alt: false,
        };
        let messages = [
            AgentMessage::Configure(RouterConfig {
                shortcuts: vec![binding],
                alt_codes,
                excluded_devices: vec!["Macro pad".to_string()],
            }),
            AgentMessage::SetAllowRightAlt(true),
        ];

        for message in messages {
            let line = encode_message(&message).unwrap();

            assert_eq!(decode_message::<AgentMessage>(&line).unwrap(), message);
        }
    }

    #[test]
    fn lines_are_framed_across_partial_reads() {
        let mut framer = LineFramer::default();

        assert!(framer.push(b"fir").unwrap().is_empty());
        assert_eq!(framer.push(b"st\nsecond\nth").unwrap(), ["first", "second"]);
        assert_eq!(framer.push(b"ird\n").unwrap(), ["third"]);
    }

    #[test]
    fn unterminated_floods_and_binary_lines_are_rejected() {
        let flood = vec![b'x'; MAX_MESSAGE_BYTES + 1];

        assert!(matches!(
            LineFramer::default().push(&flood),
            Err(ProtocolError::TooLong(_))
        ));
        assert!(matches!(
            LineFramer::default().push(b"\xFF\n"),
            Err(ProtocolError::NotText)
        ));
    }

    #[test]
    fn oversized_and_malformed_lines_are_rejected() {
        let oversized = "x".repeat(MAX_MESSAGE_BYTES + 1);

        assert!(matches!(
            decode_message::<RouterMessage>(&oversized),
            Err(ProtocolError::TooLong(_))
        ));
        assert!(matches!(
            decode_message::<RouterMessage>(r#"{"unknown":1}"#),
            Err(ProtocolError::Malformed(_))
        ));
    }
}
