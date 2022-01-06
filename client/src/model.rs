extern crate mazeio_shared;
use mazeio_shared::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc::Receiver, Mutex, RwLock};

pub type AtomicPlayerDict = Arc<RwLock<HashMap<String, Player>>>;
use mazeio_proto::game_client::GameClient;
use tokio_stream::{wrappers::ReceiverStream, StreamExt};
use tonic::Request;

pub struct GameState {
    pub player_id: String,
    pub maze: ProtoMaze,
    pub player_dict: AtomicPlayerDict,
    pub changed_since_synced: Arc<Mutex<bool>>,
}
pub struct GameStateSynced {
    pub player_id: String,
    pub maze: ProtoMaze,
    pub player_dict: HashMap<String, Player>,
}
impl GameStateSynced {
    pub async fn update_players(&mut self, game_state: &mut GameState) {
        let player_lock = game_state.player_dict.read().await;
        self.player_dict = (*player_lock).clone();
        let mut changed_lock = game_state.changed_since_synced.lock().await;
        *changed_lock = false;
    }
}

impl GameState {
    pub async fn to_synced(&mut self) -> GameStateSynced {
        let player_lock = self.player_dict.read().await;
        let mut changed_lock = self.changed_since_synced.lock().await;
        *changed_lock = false;
        GameStateSynced {
            player_id: self.player_id.clone(),
            maze: self.maze.clone(),
            player_dict: (*player_lock).clone(),
        }
    }
    pub async fn initial_state(
        name: String,
        client: &mut GameClient<tonic::transport::Channel>,
    ) -> Result<Self, tonic::Status> {
        let join_game_response = client
            .connect_player(Request::new(JoinGameRequest { name }))
            .await?
            .into_inner();

        match join_game_response {
            JoinGameResponse {
                maze: Some(maze_val),
                players,
                player_id,
            } => Ok(GameState {
                player_id: player_id,
                maze: maze_val,
                player_dict: Arc::new(RwLock::new(
                    players
                        .iter()
                        .map(|player| (player.id.clone(), player.clone()))
                        .collect::<HashMap<String, Player>>(),
                )),
                changed_since_synced: Arc::new(Mutex::new(false)),
            }),
            _ => panic!(),
        }
    }

    pub async fn handle_player_stream(
        &self,
        rx: Receiver<InputDirection>,
        client: &mut GameClient<tonic::transport::Channel>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut player_stream = client
            .stream_game(ReceiverStream::new(rx))
            .await?
            .into_inner();

        let player_dict = self.player_dict.clone();
        let changed_since_synced = self.changed_since_synced.clone();
        tokio::spawn(async move {
            while let Some(res) = player_stream.next().await {
                if let Ok(player) = res {
                    let mut player_dict_lock = player_dict.write().await;
                    if !player.alive {
                        (*player_dict_lock).remove(&player.id);
                    } else {
                        (*player_dict_lock).insert(player.id.clone(), player);
                        //println!("{:#?}\n", (*player_dict_lock));
                    }
                    //println!("Got more player info from server!\n");
                    let mut changed_lock = changed_since_synced.lock().await;
                    *changed_lock = true;
                } else {
                    println! {"{:?}", res};
                    break;
                }
            }
        });
        Ok(())
    }
}
