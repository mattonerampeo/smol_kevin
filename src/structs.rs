use std::{collections::HashMap, sync::Arc, env};
use tokio::{
    sync::{Mutex, RwLock},
};
use serenity::{
    model::{
        prelude::{GuildId, UserId},
    },
    prelude::TypeMapKey,
};

pub struct Buffer {
    buf: Vec<i16>,
    pos: usize,
}

impl Buffer {
    pub fn new() -> Self {
        let size = buffer_size();
        Self {
            buf: vec![0; size],
            pos: 0,
        }
    }

    pub fn push(&mut self, val: &Vec<i16>) {
        let size = buffer_size();
        for bytes in val {
            self.buf[self.pos] = *bytes;
            self.pos = if self.pos < size - 1 { self.pos + 1 } else { 0 };
        }
    }

    pub fn pop(&self) -> Vec<i16> {
        let size = buffer_size();
        let start = if self.pos < size - 1 { self.pos } else { 0 };
        [&self.buf[start..], &self.buf[..start]].concat()
    }
}

pub struct Receiver {
    pub(crate) lobby: Arc<(Mutex<HashMap<u32, Buffer>>, Mutex<HashMap<u32, UserId>>)>,
}

impl Receiver {
    pub fn new(lobby: Arc<(Mutex<HashMap<u32, Buffer>>, Mutex<HashMap<u32, UserId>>)>) -> Self {
        // You can manage state here, such as a buffer of audio packet bytes so
        // you can later store them in intervals.
        Self { lobby }
    }
}

pub struct Lobbies; // void struct used to generate a typemap that holds all active lobbies

impl TypeMapKey for Lobbies {
    type Value = Arc<RwLock<HashMap<GuildId, Arc<(Mutex<HashMap<u32, Buffer>>, Mutex<HashMap<u32, UserId>>)>>>>; // a game is held within a lobby. the text channel id is the lobby's unique code
}

fn buffer_size () -> usize {
    match env::var("DISCORD_BUFFER_SIZE") {
        Ok(custom_size) => custom_size.parse::<usize>()
            .expect("make sure the custom buffer is valid!"),
        Err(_) => 1440000
    }
}


