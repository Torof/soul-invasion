//! Client <-> server protocol for Soul Invasion.
//!
//! Architecture decisions D25 and D26: WebSocket transport, JSON encoding
//! confined to this crate (and to one module on the client side), every
//! message carries its protocol version. Clients only ever send
//! *intentions*; the server only ever sends what the player's body perceives.
//!
//! Any change to this crate is a protected area (see `CODEOWNERS`).

use serde::{Deserialize, Serialize};

/// Protocol version. Bump on any incompatible change.
pub const PROTOCOL_VERSION: u32 = 1;

/// A tile on the map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Terrain {
    /// Open ground.
    Grass,
    /// Trees.
    Forest,
    /// The river; not walkable.
    Water,
    /// A shallow crossing of the river.
    Ford,
    /// The cliff; not walkable.
    Rock,
}

/// A body as the client sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BodyState {
    /// Body id.
    pub id: u32,
    /// Column.
    pub x: i32,
    /// Row.
    pub y: i32,
}

/// Messages sent by the client to the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// First message after connecting.
    Hello {
        /// Protocol version the client speaks.
        protocol_version: u32,
    },
    /// Liveness check.
    Ping {
        /// Echoed back in [`ServerMessage::Pong`].
        nonce: u64,
    },
    /// Intention: walk the piloted body to a tile.
    MoveTo {
        /// Column.
        x: i32,
        /// Row.
        y: i32,
    },
}

/// Messages sent by the server to the client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Reply to [`ClientMessage::Hello`].
    Welcome {
        /// Protocol version the server speaks.
        protocol_version: u32,
        /// Current world tick.
        tick: u64,
        /// The body this connection pilots, or `None` to only watch.
        body: Option<u32>,
    },
    /// Everything the client needs to draw the world, sent once after
    /// [`ServerMessage::Welcome`]. Milestone 1 sends the whole valley;
    /// interest management (D08) will narrow it later.
    WorldState {
        /// Map columns.
        width: u32,
        /// Map rows.
        height: u32,
        /// Tiles, row by row from the north-west corner.
        tiles: Vec<Terrain>,
        /// Every body.
        bodies: Vec<BodyState>,
    },
    /// Reply to [`ClientMessage::Ping`].
    Pong {
        /// The nonce from the ping.
        nonce: u64,
    },
    /// The client spoke a protocol version the server does not support.
    /// The server closes the connection after sending it.
    ProtocolMismatch {
        /// Version the server expected.
        expected: u32,
    },
    /// A new game day began.
    DayStarted {
        /// The day that just started (days count from 1).
        day: u64,
        /// The tick at which it started.
        tick: u64,
    },
    /// A body stepped onto a tile.
    BodyMoved {
        /// Who.
        body: u32,
        /// Column.
        x: i32,
        /// Row.
        y: i32,
        /// When.
        tick: u64,
    },
    /// A walk could not start (impassable or unreachable target). Sent to
    /// the pilot who asked, and to nobody else.
    MoveRefused {
        /// Who.
        body: u32,
        /// Column asked.
        x: i32,
        /// Row asked.
        y: i32,
    },
}

/// Encodes a message to its JSON wire form.
pub fn encode<T: Serialize>(message: &T) -> Result<String, serde_json::Error> {
    serde_json::to_string(message)
}

/// Decodes a message from its JSON wire form.
pub fn decode<'a, T: Deserialize<'a>>(text: &'a str) -> Result<T, serde_json::Error> {
    serde_json::from_str(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_messages_round_trip() {
        let messages = [
            ClientMessage::Hello {
                protocol_version: PROTOCOL_VERSION,
            },
            ClientMessage::Ping { nonce: 42 },
            ClientMessage::MoveTo { x: 3, y: -1 },
        ];
        for message in messages {
            let text = encode(&message).unwrap();
            let back: ClientMessage = decode(&text).unwrap();
            assert_eq!(message, back);
        }
    }

    #[test]
    fn server_messages_round_trip() {
        let messages = [
            ServerMessage::Welcome {
                protocol_version: PROTOCOL_VERSION,
                tick: 7,
                body: Some(1),
            },
            ServerMessage::Welcome {
                protocol_version: PROTOCOL_VERSION,
                tick: 7,
                body: None,
            },
            ServerMessage::WorldState {
                width: 2,
                height: 1,
                tiles: vec![Terrain::Grass, Terrain::Water],
                bodies: vec![BodyState { id: 1, x: 0, y: 0 }],
            },
            ServerMessage::Pong { nonce: 42 },
            ServerMessage::ProtocolMismatch { expected: 1 },
            ServerMessage::DayStarted {
                day: 2,
                tick: 72_000,
            },
            ServerMessage::BodyMoved {
                body: 1,
                x: 4,
                y: 5,
                tick: 9,
            },
            ServerMessage::MoveRefused {
                body: 1,
                x: 4,
                y: 5,
            },
        ];
        for message in messages {
            let text = encode(&message).unwrap();
            let back: ServerMessage = decode(&text).unwrap();
            assert_eq!(message, back);
        }
    }

    #[test]
    fn wire_format_is_tagged_snake_case() {
        let text = encode(&ClientMessage::Ping { nonce: 1 }).unwrap();
        assert_eq!(text, r#"{"type":"ping","nonce":1}"#);
        let text = encode(&ClientMessage::MoveTo { x: 2, y: 3 }).unwrap();
        assert_eq!(text, r#"{"type":"move_to","x":2,"y":3}"#);
        let text = encode(&ServerMessage::WorldState {
            width: 1,
            height: 1,
            tiles: vec![Terrain::Ford],
            bodies: vec![],
        })
        .unwrap();
        assert_eq!(
            text,
            r#"{"type":"world_state","width":1,"height":1,"tiles":["ford"],"bodies":[]}"#
        );
    }

    #[test]
    fn unknown_messages_are_rejected() {
        assert!(decode::<ClientMessage>(r#"{"type":"teleport"}"#).is_err());
        assert!(decode::<ClientMessage>("not json").is_err());
    }
}
