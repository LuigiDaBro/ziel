use std::error;

use tokio::{io, net};

use crate::{logic, prot};

#[derive(thiserror::Error, Debug)]
pub enum Error<I: UI> {
    #[error("protocol error: {0}")]
    Protocol(#[from] prot::Error),
    #[error("interface error: {0}")]
    Interface(#[from] UIError<I::Error>),
    #[error("networking error: {0}")]
    Networking(#[from] io::Error),
}

pub struct ClientInfo<'i> {
    pub ships: &'i [logic::Ship; 5],
    pub selfhits: &'i [[Option<logic::AttackInfo>; 10]; 10],
    pub opphits: &'i [[Option<logic::AttackInfo>; 10]; 10],

    pub message: &'i [Message],
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Message {
    SuccessfullyConnected,
    SelectTarget,
    WaitForOpp,
    ShipHit,
    ShipSunken,
    ShipMissed,
    OppShipHit,
    OppShipSunken,
    OppShipMissed,
}

pub struct Client {
    ships: logic::Ships,
    selfhits: [[Option<logic::AttackInfo>; 10]; 10],
    opphits: [[Option<logic::AttackInfo>; 10]; 10],

    stream: net::TcpStream,
    message: Vec<Message>,
}

#[derive(thiserror::Error, Debug)]
#[error(transparent)]
pub struct UIError<E: error::Error + 'static>(#[from] E);

pub trait UI {
    type Error: error::Error + 'static;

    fn buildboard(&mut self) -> Result<logic::Ships, UIError<Self::Error>>;
    fn displayboard(&mut self, info: ClientInfo) -> Result<(), UIError<Self::Error>>;
    fn selecttarget(&mut self, info: ClientInfo) -> Result<logic::Position, UIError<Self::Error>>;
    fn displayvictory(&mut self, info: ClientInfo) -> Result<(), UIError<Self::Error>>;
    fn displayloss(&mut self, info: ClientInfo) -> Result<(), UIError<Self::Error>>;
}

impl Client {
    fn info(&self) -> ClientInfo {
        ClientInfo {
            ships: self.ships.asarray(),
            selfhits: &self.selfhits,
            opphits: &self.opphits,
            message: &self.message,
        }
    }

    pub async fn connect<I: UI>(
        addr: impl net::ToSocketAddrs,
        interface: &mut I,
    ) -> Result<Client, Error<I>> {
        let ships = interface.buildboard()?;
        let mut stream = net::TcpStream::connect(addr).await?;

        prot::sendmessage(&mut stream, prot::ClientMessage::Handshake).await?;
        if let prot::ServerMessage::Handshake = prot::readmessage(&mut stream).await? {
        } else {
            return Err(prot::Error::UnsuccessfulHandshake.into());
        }
        Ok(Client {
            ships,
            selfhits: [[None; 10]; 10],
            opphits: [[None; 10]; 10],
            stream,
            message: vec![Message::SuccessfullyConnected],
        })
    }

    pub async fn play<I: UI>(&mut self, interface: &mut I) -> Result<bool, Error<I>> {
        interface.displayboard(self.info())?;

        let mut victory = None;
        loop {
            let request = prot::readmessage(&mut self.stream).await?;
            let response = match request {
                prot::ServerMessage::RequestShipPositions => {
                    prot::ClientMessage::ShipPositions(self.ships)
                }
                prot::ServerMessage::RequestTarget => {
                    self.message.push(Message::SelectTarget);
                    prot::ClientMessage::Target(interface.selecttarget(self.info())?)
                }
                prot::ServerMessage::Invalid => prot::ClientMessage::Acknowledge,
                prot::ServerMessage::InformTargetSelection => {
                    self.message.push(Message::WaitForOpp);
                    prot::ClientMessage::Acknowledge
                }
                prot::ServerMessage::InformTargetHitYou(pos, sunken) => {
                    self.message.push(if sunken {
                        Message::ShipSunken
                    } else {
                        Message::ShipHit
                    });
                    let (x, y) = pos.coords();
                    self.selfhits[y as usize][x as usize] = Some(logic::AttackInfo::Hit(sunken));
                    prot::ClientMessage::Acknowledge
                }
                prot::ServerMessage::InformTargetHitOpp(pos, sunken) => {
                    self.message.push(if sunken {
                        Message::OppShipSunken
                    } else {
                        Message::OppShipHit
                    });
                    let (x, y) = pos.coords();
                    self.opphits[y as usize][x as usize] = Some(logic::AttackInfo::Hit(sunken));
                    prot::ClientMessage::Acknowledge
                }
                prot::ServerMessage::InformTargetMissYou(pos) => {
                    self.message.push(Message::ShipMissed);
                    let (x, y) = pos.coords();
                    self.selfhits[y as usize][x as usize] = Some(logic::AttackInfo::Miss);
                    prot::ClientMessage::Acknowledge
                }
                prot::ServerMessage::InformTargetMissOpp(pos) => {
                    self.message.push(Message::OppShipMissed);
                    let (x, y) = pos.coords();
                    self.opphits[y as usize][x as usize] = Some(logic::AttackInfo::Miss);
                    prot::ClientMessage::Acknowledge
                }
                prot::ServerMessage::InformVictory => {
                    interface.displayvictory(self.info())?;
                    victory = Some(true);
                    prot::ClientMessage::Acknowledge
                }
                prot::ServerMessage::InformLoss => {
                    interface.displayloss(self.info())?;
                    victory = Some(false);
                    prot::ClientMessage::Acknowledge
                }
                prot::ServerMessage::TerminateConnection => {
                    prot::sendmessage(&mut self.stream, prot::ClientMessage::Acknowledge).await?;
                    return victory.ok_or(io::Error::from(io::ErrorKind::ConnectionAborted).into());
                }
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "invalid server request",
                    )
                    .into());
                }
            };
            prot::sendmessage(&mut self.stream, response).await?;
            match victory {
                Some(true) => interface.displayvictory(self.info()),
                Some(false) => interface.displayloss(self.info()),
                None => interface.displayboard(self.info()),
            }?;
        }
    }
}
