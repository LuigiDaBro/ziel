use clap::Parser;
use std::net;
use ziel::{client::Client, server, tui};

const DEFAULTADDR: net::SocketAddr =
    net::SocketAddr::new(net::IpAddr::V4(net::Ipv4Addr::new(127, 0, 0, 1)), 8080);

/// online multiplayer warship through local server
#[derive(clap::Parser)]
#[command(version, about, long_about = None)]
struct Args {
    /// the address to connect to
    #[arg(short, long, default_value_t = DEFAULTADDR)]
    addr: std::net::SocketAddr,

    /// act as server [default: client]
    #[arg(long)]
    server: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    if args.server {
        tracing_subscriber::fmt::init();
        server::listen(args.addr).await?;
    } else {
        let mut interface = tui::Interface::new();
        let mut client = Client::connect(args.addr, &mut interface).await?;
        client.play(&mut interface).await?;
    }
    Ok(())
}
