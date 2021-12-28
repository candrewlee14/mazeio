mod shared;
use async_stream::yielder::Receiver;
use shared::*;  

use mazeio_proto::game_server::{Game, GameServer};
use tokio::sync::mpsc::error::SendError;
use tokio::time::{self, Duration};

use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::{Arc};
use tokio::sync::{mpsc, RwLock, broadcast, Mutex};
use tokio_stream::wrappers::{ReceiverStream, BroadcastStream};
use tonic::{transport::Server, Request, Response, Status, Streaming};
use futures_util::{TryStreamExt, StreamExt, stream::MapErr};

use std::collections::HashMap;

type AtomicPlayerList = Arc<RwLock<Vec<RwLock<Player>>>>;
type AtomicPlayerDict = Arc<RwLock<HashMap<SocketAddr, Arc<RwLock<Player>>>>>;
type AtomicMaze = Arc<RwLock<ProtoMaze>>;

#[derive(Debug)]
pub struct GameService {
    maze: AtomicMaze,
    players: AtomicPlayerDict,
    tx: broadcast::Sender<Player>,
}
impl GameService {
    fn new(maze_width: usize, maze_height: usize) -> Self {
        let (tx, _rx) = broadcast::channel(50);
        Self {
            maze: Arc::new(RwLock::new(ProtoMaze::new(maze_width, maze_height))),
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
        let addr = request.remote_addr().unwrap();
        let join_game_request: JoinGameRequest = request.into_inner();
        // add player
        let player_id = {
            let new_player = Player::new(join_game_request.name);
            // send new player to the broadcast
            self.tx.clone().send(new_player.clone());
            let id = new_player.id.clone();
            let mut player_dict = self.players.write().await;
            (*player_dict).insert(addr, Arc::new(RwLock::new(new_player)));
            id
        };
        let players = {
            let player_dict = self.players.read().await;
            let mut ps : Vec<Player> = Vec::with_capacity((*player_dict).len());
            for player_lock in player_dict.values() {
                let player = player_lock.read().await;
                ps.push((*player).clone());
            }
            ps
        };
        let maze_data = self.maze.read().await;
        Ok(Response::new(JoinGameResponse {
            player_id: player_id,
            maze: Some((*maze_data).clone()),
            players: players
        }))
    }

    type StreamGameStream = Pin<Box<dyn futures_core::Stream<Item = Result<Player, Status>> + Send + 'static>>;
    async fn stream_game(
        &self,
        request: Request<Streaming<InputDirection>>,
    ) -> Result<Response<Self::StreamGameStream>, Status> {
        let addr = request.remote_addr().unwrap();
        let mut dir_stream = request.into_inner();
        
        // Update position
        let players_dict = self.players.clone();
        let maze = self.maze.clone();
        let broadcast_tx = self.tx.clone();
        tokio::spawn( async move {
            while let Some(dir) = dir_stream.next().await {
                let player_dict_lock = players_dict.read().await;
                let maze_lock = maze.read().await;
                let player = (*player_dict_lock)[&addr].clone();

                let mut player_lock = player.write().await; 
                (*player_lock).move_if_valid(&*maze_lock, Direction::from_i32(dir.unwrap().direction).unwrap());
                broadcast_tx.send((*player_lock).clone()).unwrap();
            }
        });

        let broadcast_sub = self.tx.subscribe();
        Ok(Response::new(
            Box::pin(
                BroadcastStream::new(broadcast_sub)
                .map_err(|_e| tonic::Status::internal("broadcast error")))))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Hello, world!");
    let addr = "[::1]:50051".parse().unwrap();
    let game = GameService::new(16, 32);
    println!("Server listening on {}", addr);
    Server::builder()
        .add_service(GameServer::new(game))
        .serve(addr)
        .await?;
    Ok(())
}
