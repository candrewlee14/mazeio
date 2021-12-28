mod shared;
use shared::*;

use mazeio_proto::game_server::{Game, GameServer};
use tokio::time::{self, Duration};

use futures_util::{stream::MapErr, StreamExt, TryStreamExt};
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio_stream::wrappers::{BroadcastStream, ReceiverStream};
use tonic::{transport::Server, Request, Response, Status, Streaming};

use std::collections::HashMap;

type AtomicPlayerDict = Arc<RwLock<HashMap<SocketAddr, Arc<RwLock<Player>>>>>;
type AtomicMaze = Arc<RwLock<ProtoMaze>>;

#[derive(Debug)]
pub struct GameService {
    maze: ProtoMaze,
    players: AtomicPlayerDict,
    tx: broadcast::Sender<Player>,
}
impl GameService {
    fn new(maze_width: usize, maze_height: usize) -> Self {
        let (tx, _rx) = broadcast::channel(50);
        Self {
            maze: ProtoMaze::new(maze_width, maze_height),
            players: Arc::new(RwLock::new(HashMap::new())),
            tx: tx,
        }
    }
}
#[tonic::async_trait]
impl Game for GameService {
    async fn connect_player(
        &self,
        request: Request<JoinGameRequest>,
    ) -> Result<Response<JoinGameResponse>, Status> {
        // addr is how we will identify clients
        let addr = request.remote_addr().unwrap();
        let join_game_request: JoinGameRequest = request.into_inner();
        let player_id = {
            let new_player = Player::new(join_game_request.name);
            // send new player to the broadcast
            match self.tx.clone().send(new_player.clone()) {
                Ok(_) => {}
                Err(_) => {}
            };
            // insert into atomic player dict
            let id = new_player.id.clone();
            let mut player_dict = self.players.write().await;
            (*player_dict).insert(addr, Arc::new(RwLock::new(new_player)));
            id
        };
        // get already-joined players (including this new one)
        let players = {
            let player_dict = self.players.read().await;
            let mut ps: Vec<Player> = Vec::with_capacity((*player_dict).len());
            for player_lock in player_dict.values() {
                let player = player_lock.read().await;
                ps.push((*player).clone());
            }
            ps
        };
        // return response
        Ok(Response::new(JoinGameResponse {
            player_id: player_id,
            maze: Some(self.maze.clone()),
            players: players,
        }))
    }

    type StreamGameStream =
        Pin<Box<dyn futures_core::Stream<Item = Result<Player, Status>> + Send + 'static>>;
    async fn stream_game(
        &self,
        request: Request<Streaming<InputDirection>>,
    ) -> Result<Response<Self::StreamGameStream>, Status> {
        let addr = request.remote_addr().unwrap();
        let mut dir_stream = request.into_inner();

        // clones for moving into thread
        {
            let players_dict = self.players.clone();
            let broadcast_tx = self.tx.clone();
            let maze = self.maze.clone();
            // read from client stream and send their new player location
            // into broadcast channel
            tokio::spawn(async move {
                while let Ok(maybe_dir) = dir_stream.try_next().await {
                    let player_dict_lock = players_dict.read().await;
                    let player = (*player_dict_lock)[&addr].clone();
                    // println!("{:?}", maybe_dir);
                    if let Some(indir) = maybe_dir {
                        let mut player_lock = player.write().await;
                        (*player_lock)
                            .move_if_valid(&maze, Direction::from_i32(indir.direction).unwrap());
                        broadcast_tx.send((*player_lock).clone()).unwrap();
                    } else {
                        break;
                    }
                }
                let mut player_dict_lock = players_dict.write().await;                
                let player = (*player_dict_lock)[&addr].clone();
                let mut player_lock = player.write().await;
                (*player_lock).alive = false;
                broadcast_tx.send((*player_lock).clone()).unwrap();
                (*player_dict_lock).remove(&addr);
            });
        }

        let broadcast_sub = self.tx.subscribe();
        Ok(Response::new(Box::pin(
            BroadcastStream::new(broadcast_sub)
                .map_err(|_e| tonic::Status::internal("Broadcast error")),
        )))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Hello, world!");
    let addr = "[::1]:50051".parse()?;
    let game = GameService::new(16, 32);
    println!("Server listening on {}", addr);
    Server::builder()
        .add_service(GameServer::new(game))
        .serve(addr)
        .await?;
    Ok(())
}
