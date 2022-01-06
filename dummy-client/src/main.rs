use clap::Parser;
use futures_util::future::join_all;
use mazeio_proto::game_client::GameClient;
use mazeio_shared::*;
use tokio_stream::wrappers::ReceiverStream;
use tonic::Request;

/// Program to run dummy clients to test server
#[derive(Parser, Debug, Clone)]
#[clap(about, version, author)]
struct Args {
    /// Number of clients to create
    #[clap(short, long, default_value_t = 15)]
    count: u16,
    /// Number of directions to send out
    #[clap(short, long, default_value_t = 100)]
    actions: u16,
    /// Delay between each action
    #[clap(short, long, default_value_t = 50)]
    delay_millis: u64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let mut handles = Vec::new();
    for i in 0..args.count {
        let handle = tokio::spawn(async move {
            let mut client = GameClient::connect("http://[::1]:50051").await.unwrap();
            let name = format!("client-#{}", i+1);

            let join_game_response = client
                .connect_player(Request::new(JoinGameRequest { name }))
                .await
                .unwrap()
                .into_inner();
            
            let maze = join_game_response.maze.unwrap();
            let players = join_game_response.players;
            let player_id = join_game_response.player_id;
            let my_player_i = players.iter().position(|p| p.id == player_id).unwrap();
            let mut my_player = players[my_player_i].clone();

            let (tx, rx) = tokio::sync::mpsc::channel(5);
            let mut _player_stream = client
                .stream_game(ReceiverStream::new(rx))
                .await
                .unwrap()
                .into_inner();

            let mut timer =
                tokio::time::interval(tokio::time::Duration::from_millis(args.delay_millis));
            for _action in 0..args.actions {
                let mut dir : Direction = rand::random();
                while !my_player.move_if_valid(&maze, dir) {
                    dir = rand::random();
                }
                tx.send(InputDirection {
                    direction: dir.into(),
                })
                .await
                .unwrap();
                timer.tick().await;
            }
            println!("Finished sending actions for client #{}", i+1);
        });
        println!("Spawned client #{}", i+1);
        handles.push(handle);
    }
    join_all(handles).await;
    Ok(())
}
