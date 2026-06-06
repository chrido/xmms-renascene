#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerState {
    Stopped,
    Playing,
    Paused,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Player {
    state: PlayerState,
    duration_ms: Option<i64>,
    bitrate: i32,
    frequency: i32,
    channels: i32,
    volume: i32,
    balance: i32,
    vis_data: [f32; 75],
    vis_data_valid: bool,
}

impl Default for Player {
    fn default() -> Self {
        Self {
            state: PlayerState::Stopped,
            duration_ms: None,
            bitrate: 0,
            frequency: 0,
            channels: 0,
            volume: 100,
            balance: 0,
            vis_data: [0.0; 75],
            vis_data_valid: false,
        }
    }
}

impl Player {
    pub fn state(&self) -> PlayerState {
        self.state
    }

    pub fn mark_playing(&mut self) {
        self.state = PlayerState::Playing;
    }

    pub fn pause(&mut self) {
        if self.state == PlayerState::Playing {
            self.state = PlayerState::Paused;
        }
    }

    pub fn unpause(&mut self) {
        if self.state == PlayerState::Paused {
            self.state = PlayerState::Playing;
        }
    }

    pub fn stop(&mut self) {
        self.state = PlayerState::Stopped;
        self.duration_ms = None;
        self.bitrate = 0;
        self.frequency = 0;
        self.channels = 0;
    }

    pub fn set_volume(&mut self, percent: i32) {
        self.volume = percent.clamp(0, 100);
    }

    pub fn volume(&self) -> i32 {
        self.volume
    }

    pub fn set_balance(&mut self, balance: i32) {
        self.balance = balance.clamp(-100, 100);
    }

    pub fn balance(&self) -> i32 {
        self.balance
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_and_balance_are_clamped_like_the_c_player() {
        let mut player = Player::default();
        player.set_volume(150);
        player.set_balance(-250);
        assert_eq!(player.volume(), 100);
        assert_eq!(player.balance(), -100);
    }

    #[test]
    fn pause_only_changes_a_playing_player() {
        let mut player = Player::default();
        player.pause();
        assert_eq!(player.state(), PlayerState::Stopped);
        player.mark_playing();
        player.pause();
        assert_eq!(player.state(), PlayerState::Paused);
        player.unpause();
        assert_eq!(player.state(), PlayerState::Playing);
    }
}
