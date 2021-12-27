mod shared;
use shared::*;

use mazeio_proto::game_client::GameClient;

use tonic::Request;

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
    Ok(())
}
