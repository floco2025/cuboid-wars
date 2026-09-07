use bincode::{Decode, Encode};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Encode, Decode, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeathTrigger {
    #[default]
    Never,
    Solo,
    Any,
    All,
}

impl DeathTrigger {
    pub fn applies(self, logged_in: usize, alive_after_death: usize) -> bool {
        logged_in > 0
            && match self {
                Self::Never => false,
                Self::Solo => logged_in == 1,
                Self::Any => true,
                Self::All => alive_after_death == 0,
            }
    }
}
