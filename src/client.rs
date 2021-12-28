mod shared;
use shared::*;

use mazeio_proto::game_client::GameClient;
use tokio_stream::wrappers::ReceiverStream;
use tonic::Request;
use futures_util::{TryStreamExt, StreamExt, stream::MapErr};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = GameClient::connect("http://[::1]:50051").await?;

    let response = client
        .connect_player(Request::new(JoinGameRequest {
            name: "test-name".to_string(),
        }))
        .await?;

    let join_game_response = response.into_inner();
    println!("My Player ID: {}", join_game_response.player_id);
    if let Some(maze) = join_game_response.maze {
        println!("Maze:\n{}", maze.to_string());
    }
    let (tx, mut rx) = tokio::sync::mpsc::channel(3);
    tokio::spawn(async move {
        for _i in 0..30 {
            tx.send(InputDirection {direction: rand::random::<Direction>().into()}).await.unwrap();
        }
    });
    let mut playerStream = client
        .stream_game(ReceiverStream::new(rx))
        .await.unwrap().into_inner();

    while let Some(res) = playerStream.next().await {
        if let Ok(val) = res {
            println!("{:?}", val);
        }
        else {
            println!{"{:?}", res};
            break;
        }
    }

    Ok(())
}
