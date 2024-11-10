use tokio::{io, net, sync::mpsc};

use crate::{logic, prot};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("protocol error; {0}")]
    Protocol(#[from] prot::Error),
    #[error("networking error; {0}")]
    Networking(#[from] io::Error),
    #[error("middleware error; requested {0:?}, got {1:?}")]
    Middleware(CommandRequest, CommandResult),
    #[error("logic error; {0}")]
    Logic(#[from] logic::Error),
}

#[derive(Debug, Clone)]
pub enum CommandRequest {
    Handshake,

    RequestShips,
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

#[derive(Debug, Clone)]
pub enum CommandResult {
    Success,
    Invalid,
    GetShips(logic::Ships),
    GetTarget(logic::Position),
}

struct Middleware {
    stream: net::TcpStream,
    serverrx: mpsc::Receiver<CommandRequest>,
    clienttx: mpsc::Sender<Result<CommandResult, Error>>,
}

impl Middleware {
    async fn handlecmd(&mut self, cmd: CommandRequest) -> Result<CommandResult, Error> {
        match cmd {
            CommandRequest::Handshake => match prot::readmessage(&mut self.stream).await? {
                prot::ClientMessage::Handshake => {
                    prot::sendmessage(&mut self.stream, prot::ServerMessage::Handshake).await?;
                    Ok(CommandResult::Success)
                }
                _ => Ok(CommandResult::Invalid),
            },
            CommandRequest::RequestShips => {
                prot::sendmessage(&mut self.stream, prot::ServerMessage::RequestShipPositions)
                    .await?;

                match prot::readmessage(&mut self.stream).await? {
                    prot::ClientMessage::ShipPositions(ships) => Ok(CommandResult::GetShips(ships)),
                    _ => Ok(CommandResult::Invalid),
                }
            }
            CommandRequest::RequestTarget => {
                prot::sendmessage(&mut self.stream, prot::ServerMessage::RequestTarget).await?;

                match prot::readmessage(&mut self.stream).await? {
                    prot::ClientMessage::Target(pos) => Ok(CommandResult::GetTarget(pos)),
                    _ => Ok(CommandResult::Invalid),
                }
            }
            CommandRequest::InformTargetSelection => {
                prot::sendmessage(&mut self.stream, prot::ServerMessage::InformTargetSelection)
                    .await?;

                match prot::readmessage(&mut self.stream).await? {
                    prot::ClientMessage::Acknowledge => Ok(CommandResult::Success),
                    _ => Ok(CommandResult::Invalid),
                }
            }
            CommandRequest::InformTargetHitYou(pos, sunken) => {
                prot::sendmessage(
                    &mut self.stream,
                    prot::ServerMessage::InformTargetHitYou(pos, sunken),
                )
                .await?;
                match prot::readmessage(&mut self.stream).await? {
                    prot::ClientMessage::Acknowledge => Ok(CommandResult::Success),
                    _ => Ok(CommandResult::Invalid),
                }
            }
            CommandRequest::InformTargetHitOpp(pos, sunken) => {
                prot::sendmessage(
                    &mut self.stream,
                    prot::ServerMessage::InformTargetHitOpp(pos, sunken),
                )
                .await?;
                match prot::readmessage(&mut self.stream).await? {
                    prot::ClientMessage::Acknowledge => Ok(CommandResult::Success),
                    _ => Ok(CommandResult::Invalid),
                }
            }
            CommandRequest::InformTargetMissYou(pos) => {
                prot::sendmessage(
                    &mut self.stream,
                    prot::ServerMessage::InformTargetMissYou(pos),
                )
                .await?;
                match prot::readmessage(&mut self.stream).await? {
                    prot::ClientMessage::Acknowledge => Ok(CommandResult::Success),
                    _ => Ok(CommandResult::Invalid),
                }
            }
            CommandRequest::InformTargetMissOpp(pos) => {
                prot::sendmessage(
                    &mut self.stream,
                    prot::ServerMessage::InformTargetMissOpp(pos),
                )
                .await?;
                match prot::readmessage(&mut self.stream).await? {
                    prot::ClientMessage::Acknowledge => Ok(CommandResult::Success),
                    _ => Ok(CommandResult::Invalid),
                }
            }
            CommandRequest::InformVictory => {
                prot::sendmessage(&mut self.stream, prot::ServerMessage::InformVictory).await?;
                match prot::readmessage(&mut self.stream).await? {
                    prot::ClientMessage::Acknowledge => Ok(CommandResult::Success),
                    _ => Ok(CommandResult::Invalid),
                }
            }
            CommandRequest::InformLoss => {
                prot::sendmessage(&mut self.stream, prot::ServerMessage::InformLoss).await?;
                match prot::readmessage(&mut self.stream).await? {
                    prot::ClientMessage::Acknowledge => Ok(CommandResult::Success),
                    _ => Ok(CommandResult::Invalid),
                }
            }
            CommandRequest::TerminateConnection => {
                prot::sendmessage(&mut self.stream, prot::ServerMessage::TerminateConnection)
                    .await?;
                match prot::readmessage(&mut self.stream).await? {
                    prot::ClientMessage::Acknowledge => Ok(CommandResult::Success),
                    _ => Ok(CommandResult::Invalid),
                }
            }
        }
    }

    async fn run(mut self) {
        while let Some(cmd) = self.serverrx.recv().await {
            let cmdres = self.handlecmd(cmd).await;
            let _ = self.clienttx.send(cmdres).await;
        }
    }
}

pub struct Instance {
    turn: u8,
    boards: [logic::Board; 2],
    senders: [mpsc::Sender<CommandRequest>; 2],
    receivers: [mpsc::Receiver<Result<CommandResult, Error>>; 2],
}

impl Instance {
    async fn run(
        mut senders: [mpsc::Sender<CommandRequest>; 2],
        mut receivers: [mpsc::Receiver<Result<CommandResult, Error>>; 2],
    ) -> Result<(), Error> {
        for sender in &senders {
            sender.send(CommandRequest::Handshake).await.unwrap();
        }

        for receiver in &mut receivers {
            if matches!(receiver.recv().await.unwrap()?, CommandResult::Invalid) {
                return Err(prot::Error::UnsuccessfulHandshake.into());
            }
        }

        let [rx1, rx2] = &mut receivers;
        let [tx1, tx2] = &mut senders;

        let (ship1, ship2) =
            tokio::join!(Instance::getships(tx1, rx1), Instance::getships(tx2, rx2),);

        Instance {
            turn: 0,
            boards: [logic::Board::new(ship1?), logic::Board::new(ship2?)],
            senders,
            receivers,
        }
        .play()
        .await
    }

    async fn gettarget(
        txplayer: &mut mpsc::Sender<CommandRequest>,
        txopp: &mut mpsc::Sender<CommandRequest>,
        rxplayer: &mut mpsc::Receiver<Result<CommandResult, Error>>,
        rxopp: &mut mpsc::Receiver<Result<CommandResult, Error>>,
    ) -> Result<logic::Position, Error> {
        let (target, acknowledged) = tokio::join!(
            async {
                txplayer.send(CommandRequest::RequestTarget).await.unwrap();
                let res = rxplayer.recv().await.unwrap()?;
                match res {
                    CommandResult::GetTarget(target) => Ok(target),
                    other => Err(Error::Middleware(CommandRequest::RequestShips, other)),
                }
            },
            Instance::informmw(rxopp, txopp, CommandRequest::InformTargetSelection)
        );

        acknowledged?;
        target
    }

    async fn getships(
        tx: &mut mpsc::Sender<CommandRequest>,
        rx: &mut mpsc::Receiver<Result<CommandResult, Error>>,
    ) -> Result<logic::Ships, Error> {
        {
            tx.send(CommandRequest::RequestShips).await.unwrap();
            match rx.recv().await.unwrap()? {
                CommandResult::GetShips(ships) => Ok(ships),
                other => Err(Error::Middleware(CommandRequest::RequestShips, other)),
            }
        }
    }

    fn getplayeropppair<T>(turn: u8, arr: &mut [T; 2]) -> (&mut T, &mut T) {
        let [elem1, elem2] = arr;
        if turn % 2 == 0 {
            (elem1, elem2)
        } else {
            (elem2, elem1)
        }
    }

    async fn informmw(
        rx: &mut mpsc::Receiver<Result<CommandResult, Error>>,
        tx: &mut mpsc::Sender<CommandRequest>,
        cmd: CommandRequest,
    ) -> Result<(), Error> {
        tx.send(cmd.clone()).await.unwrap();
        let res = rx.recv().await.unwrap()?;
        match res {
            CommandResult::Success => Ok(()),
            other => Err(Error::Middleware(cmd, other)),
        }
    }

    async fn playturn(&mut self) -> Result<bool, Error> {
        let (_boardplayer, boardopp) = Instance::getplayeropppair(self.turn, &mut self.boards);
        let (rxplayer, rxopp) = Instance::getplayeropppair(self.turn, &mut self.receivers);
        let (txplayer, txopp) = Instance::getplayeropppair(self.turn, &mut self.senders);

        let target = Instance::gettarget(txplayer, txopp, rxplayer, rxopp).await?;
        let info = match boardopp.target(target) {
            Some(info) => info,
            None => return Err(Error::Logic(logic::Error::OccupiedTargetPosition)),
        };
        match info {
            logic::AttackInfo::Miss => {
                let (success1, success2) = tokio::join!(
                    Instance::informmw(
                        rxplayer,
                        txplayer,
                        CommandRequest::InformTargetMissOpp(target)
                    ),
                    Instance::informmw(rxopp, txopp, CommandRequest::InformTargetMissYou(target)),
                );
                success1?;
                success2?;
                self.turn += 1;
                Ok(true)
            }
            logic::AttackInfo::Hit(sunken) => {
                let (success1, success2) = tokio::join!(
                    Instance::informmw(
                        rxplayer,
                        txplayer,
                        CommandRequest::InformTargetHitOpp(target, sunken)
                    ),
                    Instance::informmw(
                        rxopp,
                        txopp,
                        CommandRequest::InformTargetHitYou(target, sunken)
                    ),
                );
                success1?;
                success2?;

                if boardopp.allsunken() {
                    let (success1, success2) = tokio::join!(
                        Instance::informmw(rxplayer, txplayer, CommandRequest::InformVictory),
                        Instance::informmw(rxopp, txopp, CommandRequest::InformLoss),
                    );
                    success1?;
                    success2?;

                    let (success1, success2) = tokio::join!(
                        Instance::informmw(rxplayer, txplayer, CommandRequest::TerminateConnection),
                        Instance::informmw(rxopp, txopp, CommandRequest::TerminateConnection),
                    );
                    success1?;
                    success2?;
                    Ok(false)
                } else {
                    Ok(true)
                }
            }
        }
    }

    async fn play(mut self) -> Result<(), Error> {
        loop {
            match self.playturn().await {
                Ok(true) => continue,
                Ok(false) => break Ok(()),
                Err(err) => break Err(err),
            }
        }?;

        let [rx1, rx2] = &mut self.receivers;
        let [tx1, tx2] = &mut self.senders;
        let _ = tokio::join!(
            Instance::informmw(rx1, tx1, CommandRequest::TerminateConnection),
            Instance::informmw(rx2, tx2, CommandRequest::TerminateConnection),
        );
        Ok(())
    }
}

pub async fn listen(addr: impl net::ToSocketAddrs) -> io::Result<()> {
    tracing::info!("LISTENING");

    let listener = net::TcpListener::bind(addr).await?;
    loop {
        let (stream1, _) = listener.accept().await?;
        tracing::info!("player one connected");
        let (stream2, _) = listener.accept().await?;
        tracing::info!("player two connected");

        let (txcs1, rxcs1) = mpsc::channel(10);
        let (txsc1, rxsc1) = mpsc::channel(10);

        let mw1 = Middleware {
            stream: stream1,
            serverrx: rxsc1,
            clienttx: txcs1,
        };

        let (txcs2, rxcs2) = mpsc::channel(10);
        let (txsc2, rxsc2) = mpsc::channel(10);

        let mw2 = Middleware {
            stream: stream2,
            serverrx: rxsc2,
            clienttx: txcs2,
        };

        tracing::info!("ready to play");
        let client1 = tokio::spawn(async move { Middleware::run(mw1).await });
        let client2 = tokio::spawn(async move { Middleware::run(mw2).await });
        let instance =
            tokio::spawn(async move { Instance::run([txsc1, txsc2], [rxcs1, rxcs2]).await });

        let (_, _, instanceres) = tokio::join!(client1, client2, instance);
        match instanceres {
            Ok(Ok(())) => tracing::info!("successful game"),
            Ok(Err(err)) => tracing::warn!("error finishing game; {err}"),
            Err(err) => tracing::error!("error joining game; {err}"),
        }
    }
}
