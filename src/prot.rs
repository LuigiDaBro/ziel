use std::array;
use tokio::{
    io::{self, AsyncReadExt, AsyncWriteExt},
    net,
};

use crate::logic;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("invalid message; typemarker: {typemarker}, sizemarker: {sizemarker}, body: {body:?}")]
    Message {
        typemarker: u8,
        sizemarker: u32,
        body: Vec<u8>,
    },
    #[error("{0}")]
    Networking(#[from] io::Error),
    #[error("unsuccessful handshake")]
    UnsuccessfulHandshake,
}

impl From<RawMessage> for Error {
    fn from(value: RawMessage) -> Error {
        Error::Message {
            typemarker: value.typemarker,
            sizemarker: value.body.len() as u32,
            body: value.body,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct RawMessageRef<'b> {
    pub typemarker: u8,
    pub body: &'b [u8],
}

pub struct RawMessage {
    pub typemarker: u8,
    pub body: Vec<u8>,
}

impl RawMessage {
    pub fn as_ref(&self) -> RawMessageRef {
        RawMessageRef {
            typemarker: self.typemarker,
            body: &self.body,
        }
    }
}

impl<'b> RawMessageRef<'b> {
    pub fn to_owned(self) -> RawMessage {
        RawMessage {
            typemarker: self.typemarker,
            body: self.body.to_owned(),
        }
    }
}

#[derive(Debug)]
pub enum ClientMessage {
    Handshake,

    Acknowledge,

    ShipPositions(logic::Ships),
    Target(logic::Position),
}

#[derive(Debug)]
pub enum ServerMessage {
    Handshake,

    Invalid,

    RequestShipPositions,
    RequestTarget,

    InformTargetSelection,
    InformTargetHitYou(logic::Position, bool),
    InformTargetMissYou(logic::Position),
    InformTargetHitOpp(logic::Position, bool),
    InformTargetMissOpp(logic::Position),
    InformVictory,
    InformLoss,

    TerminateConnection,
}

// STREAM HANDLING  000..100
// LOGIC  HANDLING  100..150
// LOGIC  INFORMING 150..200

// FRM       SERVER | CLIENT
// 001 HANDSHAKE    | HANDSHAKE
// 002              | ACKNOWLEDGMENT
// 003 INVALID      |
// 004 TERMINATE    |
// -----------------|----------------
// 100 REQ. SHIPS   | RET. SHIPS
// 101 REQ. TARGET  | RET. TARGET
// -----------------|----------------
// 150 TARG. SELEC. |
// 151 TARG. MISS   |
// 152 TARG. HIT    |
// 153 VICTORY      |
// 154 LOSS         |

const HANDSHAKE: RawMessageRef = RawMessageRef {
    typemarker: 1,
    body: b"HELO",
};
const ACKNOWLEDGMENT: RawMessageRef = RawMessageRef {
    typemarker: 2,
    body: b"ACK",
};
const INVALID: RawMessageRef = RawMessageRef {
    typemarker: 3,
    body: b"INVALID",
};
const TERMINATECONNECTION: RawMessageRef = RawMessageRef {
    typemarker: 4,
    body: b"TERM",
};

const SHIPPOSITIONS: u8 = 100;
const REQUESTSHIPPOSITIONS: RawMessageRef = RawMessageRef {
    typemarker: SHIPPOSITIONS,
    body: b"REQ SHIPP",
};
const TARGET: u8 = 101;
const REQUESTTARGET: RawMessageRef = RawMessageRef {
    typemarker: TARGET,
    body: b"TARG",
};

const INFORMTARGETSELECTION: RawMessageRef = RawMessageRef {
    typemarker: 150,
    body: b"INFO TARG",
};
const INFORMTARGETHIT: u8 = 151;
const INFORMTARGETMISS: u8 = 152;
const INFORMVICTORY: RawMessageRef = RawMessageRef {
    typemarker: 153,
    body: b"VICTORY",
};
const INFORMLOSS: RawMessageRef = RawMessageRef {
    typemarker: 154,
    body: b"LOSS",
};

impl TryFrom<RawMessage> for ClientMessage {
    type Error = Error;

    fn try_from(message: RawMessage) -> Result<Self, Self::Error> {
        match message.as_ref() {
            HANDSHAKE => Ok(ClientMessage::Handshake),
            ACKNOWLEDGMENT => Ok(ClientMessage::Acknowledge),
            RawMessageRef {
                typemarker: SHIPPOSITIONS,
                body,
            } if body.len() == 15 => {
                let mut error = false;
                let positions = array::from_fn(|i| {
                    let horizontal = body[i * 3] != 0;
                    let pos = logic::Position::frombyte(body[i * 3 + 1]).unwrap_or_else(|| {
                        error = true;
                        logic::Position::default()
                    });
                    let len = body[i * 3 + 2];

                    let shipplan = if horizontal {
                        logic::ShipPlan::Horizontal { pos, len }
                    } else {
                        logic::ShipPlan::Vertical { pos, len }
                    };
                    logic::Ship::try_from(shipplan).unwrap_or_else(|_| {
                        error = true;
                        unsafe {
                            // SAFE: error gets caught below
                            std::mem::transmute(shipplan)
                        }
                    })
                });

                if error {
                    return Err(Error::from(message));
                }

                Ok(ClientMessage::ShipPositions(
                    logic::Ships::try_from(positions).map_err(|_| Error::from(message))?,
                ))
            }
            RawMessageRef {
                typemarker: TARGET,
                body: [position],
            } => Ok(ClientMessage::Target(
                logic::Position::frombyte(*position).ok_or(Error::from(message))?,
            )),
            _ => Err(Error::from(message)),
        }
    }
}

impl From<ClientMessage> for RawMessage {
    fn from(message: ClientMessage) -> RawMessage {
        match message {
            ClientMessage::Handshake => HANDSHAKE.to_owned(),
            ClientMessage::Acknowledge => ACKNOWLEDGMENT.to_owned(),
            ClientMessage::ShipPositions(ships) => {
                let mut buffer = vec![0; 15];
                for (i, ship) in ships.into_iter().enumerate() {
                    match ship.into() {
                        logic::ShipPlan::Horizontal { pos, len } => {
                            buffer[i * 3] = true as u8;
                            buffer[i * 3 + 1] = pos.byte();
                            buffer[i * 3 + 2] = len;
                        }
                        logic::ShipPlan::Vertical { pos, len } => {
                            buffer[i * 3] = false as u8;
                            buffer[i * 3 + 1] = pos.byte();
                            buffer[i * 3 + 2] = len;
                        }
                    }
                }
                RawMessage {
                    typemarker: SHIPPOSITIONS,
                    body: buffer,
                }
            }
            ClientMessage::Target(pos) => RawMessage {
                typemarker: TARGET,
                body: vec![pos.byte()],
            },
        }
    }
}

impl TryFrom<RawMessage> for ServerMessage {
    type Error = Error;

    fn try_from(message: RawMessage) -> Result<Self, Self::Error> {
        match message.as_ref() {
            HANDSHAKE => Ok(ServerMessage::Handshake),
            INVALID => Ok(ServerMessage::Invalid),
            REQUESTSHIPPOSITIONS => Ok(ServerMessage::RequestShipPositions),
            REQUESTTARGET => Ok(ServerMessage::RequestTarget),
            RawMessageRef {
                typemarker: INFORMTARGETHIT,
                body: [0, pos, sunken],
            } => {
                let sunken = *sunken != 0;
                let pos = logic::Position::frombyte(*pos).ok_or(Error::from(message))?;
                Ok(ServerMessage::InformTargetHitYou(pos, sunken))
            }
            RawMessageRef {
                typemarker: INFORMTARGETHIT,
                body: [1, pos, sunken],
            } => {
                let sunken = *sunken != 0;
                let pos = logic::Position::frombyte(*pos).ok_or(Error::from(message))?;
                Ok(ServerMessage::InformTargetHitOpp(pos, sunken))
            }
            RawMessageRef {
                typemarker: INFORMTARGETMISS,
                body: [0, pos],
            } => Ok(ServerMessage::InformTargetMissYou(
                logic::Position::frombyte(*pos).ok_or(Error::from(message))?,
            )),
            RawMessageRef {
                typemarker: INFORMTARGETMISS,
                body: [1, pos],
            } => Ok(ServerMessage::InformTargetMissOpp(
                logic::Position::frombyte(*pos).ok_or(Error::from(message))?,
            )),
            INFORMTARGETSELECTION => Ok(ServerMessage::InformTargetSelection),
            INFORMVICTORY => Ok(ServerMessage::InformVictory),
            INFORMLOSS => Ok(ServerMessage::InformLoss),
            TERMINATECONNECTION => Ok(ServerMessage::TerminateConnection),
            _ => Err(Error::from(message)),
        }
    }
}

impl From<ServerMessage> for RawMessage {
    fn from(message: ServerMessage) -> Self {
        match message {
            ServerMessage::Handshake => HANDSHAKE.to_owned(),
            ServerMessage::Invalid => INVALID.to_owned(),
            ServerMessage::RequestTarget => REQUESTTARGET.to_owned(),
            ServerMessage::RequestShipPositions => REQUESTSHIPPOSITIONS.to_owned(),
            ServerMessage::InformTargetHitYou(pos, sunken) => RawMessage {
                typemarker: INFORMTARGETHIT,
                body: vec![0, pos.byte(), sunken as u8],
            },
            ServerMessage::InformTargetHitOpp(pos, sunken) => RawMessage {
                typemarker: INFORMTARGETHIT,
                body: vec![1, pos.byte(), sunken as u8],
            },
            ServerMessage::InformTargetMissYou(pos) => RawMessage {
                typemarker: INFORMTARGETMISS,
                body: vec![0, pos.byte()],
            },
            ServerMessage::InformTargetMissOpp(pos) => RawMessage {
                typemarker: INFORMTARGETMISS,
                body: vec![1, pos.byte()],
            },
            ServerMessage::InformVictory => INFORMVICTORY.to_owned(),
            ServerMessage::InformLoss => INFORMLOSS.to_owned(),
            ServerMessage::InformTargetSelection => INFORMTARGETSELECTION.to_owned(),
            ServerMessage::TerminateConnection => TERMINATECONNECTION.to_owned(),
        }
    }
}

pub async fn readmessage<M>(stream: &mut net::TcpStream) -> Result<M, Error>
where
    M: TryFrom<RawMessage, Error = Error>,
{
    let mut typemarker = [0u8; 1];
    let mut sizemarker = [0u8; 4];
    stream.read_exact(&mut typemarker).await?;
    stream.read_exact(&mut sizemarker).await?;
    let typemarker = typemarker[0];
    let sizemarker = u32::from_le_bytes(sizemarker);
    let mut body = vec![0u8; sizemarker as usize];
    stream.read_exact(&mut body).await?;
    let raw = RawMessage { typemarker, body };
    M::try_from(raw)
}

pub async fn sendmessage<M>(stream: &mut net::TcpStream, message: M) -> Result<(), Error>
where
    RawMessage: From<M>,
{
    let message = RawMessage::from(message);
    let typemarker = [message.typemarker; 1];
    let sizemarker = u32::to_le_bytes(message.body.len() as u32);
    let body = message.body;
    stream.write_all(&typemarker).await?;
    stream.write_all(&sizemarker).await?;
    stream.write_all(&body).await?;
    stream.flush().await?;

    Ok(())
}
