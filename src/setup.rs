use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct BotConfig {
    pub set_up_commands: bool,
    pub bot_token: String,
    pub app_id: u64,
}

/// `BotConfig` implements `Default`
impl ::std::default::Default for BotConfig {
    fn default() -> Self {
        let cfg = Self {
            set_up_commands: false,
            bot_token: "test_token".into(),
            app_id: 0,
        };
        confy::store("smol_kevin", cfg).expect("Could not save default config. Aborting...");
        Self {
            set_up_commands: false,
            bot_token: "test_token".into(),
            app_id: 0,
        }
    }
}

pub fn load_bot_config() -> BotConfig {
    confy::load("smol_kevin").expect("Config is unreachable. Aborting...")
}
