//! Client <-> server protocol for Soul Invasion.
//!
//! Architecture decisions D25 and D26: WebSocket transport, JSON encoding
//! confined to this crate (and to one module on the client side), every
//! message carries its protocol version. Clients only ever send
//! *intentions*; the server only ever sends what the player's body perceives.
//!
//! The ground travels in chunks (D08, D31): after [`ServerMessage::Welcome`]
//! and [`ServerMessage::WorldState`], the client receives one
//! [`ServerMessage::Chunk`] per chunk around its body, then more as the
//! body walks and [`ServerMessage::ChunkHidden`] for those left behind.
//! Bodies are only reported while they stand in a chunk the client holds.
//!
//! Any change to this crate is a protected area (see `CODEOWNERS`).

use serde::{Deserialize, Serialize};

/// Protocol version. Bump on any incompatible change.
///
/// - v2: the ground travels in chunks around the body instead of whole.
/// - v3: chunks carry the items lying in them; `world_state` carries the
///   catalogue of materials they are made of.
pub const PROTOCOL_VERSION: u32 = 3;

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

/// A material as the client needs to know it: a name to show and a
/// provisional look (D27: the client never reads the content files).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaterialView {
    /// Content id, the key items refer to.
    pub id: String,
    /// Name shown to players.
    pub name: String,
    /// `#rrggbb`.
    pub colour: String,
}

/// A piece of material lying on the ground, as the client sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemState {
    /// Item id.
    pub id: u64,
    /// Column.
    pub x: i32,
    /// Row.
    pub y: i32,
    /// Content id of its material, found in [`ServerMessage::WorldState`].
    pub material: String,
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
    /// The shape of the world, sent once after [`ServerMessage::Welcome`].
    /// The ground itself follows in [`ServerMessage::Chunk`] messages.
    WorldState {
        /// Map columns.
        width: u32,
        /// Map rows.
        height: u32,
        /// Side of a chunk, in tiles.
        chunk_size: u32,
        /// Every material items may be made of.
        materials: Vec<MaterialView>,
    },
    /// A chunk of ground the client now holds, with the bodies standing
    /// in it and the items lying in it. Chunk (0, 0) holds tiles
    /// `(0..chunk_size, 0..chunk_size)`.
    Chunk {
        /// Column of chunks.
        x: i32,
        /// Row of chunks.
        y: i32,
        /// `chunk_size × chunk_size` tiles, row by row from the north-west
        /// corner; rock where the chunk reaches past the map.
        tiles: Vec<Terrain>,
        /// Every body standing in this chunk.
        bodies: Vec<BodyState>,
        /// Every item lying in this chunk.
        items: Vec<ItemState>,
    },
    /// A chunk the client no longer holds: forget its tiles and the
    /// bodies in it.
    ChunkHidden {
        /// Column of chunks.
        x: i32,
        /// Row of chunks.
        y: i32,
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
    /// A body stepped onto a tile of a chunk the client holds.
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
    /// A body walked out of the chunks the client holds: forget it.
    BodyLeft {
        /// Who.
        body: u32,
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
                width: 256,
                height: 256,
                chunk_size: 32,
                materials: vec![MaterialView {
                    id: "flint".into(),
                    name: "Flint".into(),
                    colour: "#5e5a52".into(),
                }],
            },
            ServerMessage::Chunk {
                x: 5,
                y: -1,
                tiles: vec![Terrain::Grass, Terrain::Water],
                bodies: vec![BodyState {
                    id: 1,
                    x: 160,
                    y: 0,
                }],
                items: vec![ItemState {
                    id: 9,
                    x: 161,
                    y: 1,
                    material: "flint".into(),
                }],
            },
            ServerMessage::ChunkHidden { x: 5, y: 4 },
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
            ServerMessage::BodyLeft { body: 1 },
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
        let text = encode(&ServerMessage::Chunk {
            x: 1,
            y: 2,
            tiles: vec![Terrain::Ford],
            bodies: vec![],
            items: vec![ItemState {
                id: 3,
                x: 32,
                y: 64,
                material: "pebble".into(),
            }],
        })
        .unwrap();
        assert_eq!(
            text,
            r#"{"type":"chunk","x":1,"y":2,"tiles":["ford"],"bodies":[],"items":[{"id":3,"x":32,"y":64,"material":"pebble"}]}"#
        );
    }

    #[test]
    fn unknown_messages_are_rejected() {
        assert!(decode::<ClientMessage>(r#"{"type":"teleport"}"#).is_err());
        assert!(decode::<ClientMessage>("not json").is_err());
    }
}
