//! JSON-lines socket control server for frontends and E2E tests.
//!
//! The protocol is intentionally small and telnet-friendly: each input line is a
//! JSON object, and each response is one JSON object followed by `\n`.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use serde_json::{json, Value};

use crate::app::command::{AppCommand, AudioCommand, PanelCommand, PlayerCommand, PlaylistCommand};

#[derive(Debug, Clone, PartialEq)]
pub enum SocketCommand {
    App(AppCommand),
    Ui(SocketUiCommand),
    Quit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SocketUiCommand {
    SetPreferencesVisible(bool),
    TogglePreferences,
    SetMainMenuVisible(bool),
    SetSkinBrowserVisible(bool),
    ToggleSkinBrowser,
}

#[derive(Debug)]
pub struct SocketRequest {
    pub id: Option<Value>,
    pub command: SocketCommand,
    reply: Sender<SocketAck>,
}

impl SocketRequest {
    pub fn accept(self) {
        let _ = self.reply.send(SocketAck::accepted());
    }

    pub fn reject(self, error: impl Into<String>) {
        let _ = self.reply.send(SocketAck::rejected(error));
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SocketAck {
    pub accepted: bool,
    pub error: Option<String>,
}

impl SocketAck {
    pub fn accepted() -> Self {
        Self {
            accepted: true,
            error: None,
        }
    }

    pub fn rejected(error: impl Into<String>) -> Self {
        Self {
            accepted: false,
            error: Some(error.into()),
        }
    }
}

pub struct SocketControl {
    receiver: Receiver<SocketRequest>,
}

impl SocketControl {
    pub fn try_recv(&self) -> Option<SocketRequest> {
        self.receiver.try_recv().ok()
    }
}

pub fn start_socket_control(port: u16) -> Result<SocketControl, String> {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .map_err(|err| format!("failed to bind control socket on 127.0.0.1:{port}: {err}"))?;
    listener
        .set_nonblocking(false)
        .map_err(|err| format!("failed to configure control socket: {err}"))?;
    let (sender, receiver) = mpsc::channel();
    thread::Builder::new()
        .name("xmms-control-socket".to_string())
        .spawn(move || accept_loop(listener, sender))
        .map_err(|err| format!("failed to start control socket thread: {err}"))?;
    Ok(SocketControl { receiver })
}

fn accept_loop(listener: TcpListener, sender: Sender<SocketRequest>) {
    for stream in listener.incoming() {
        let Ok(stream) = stream else {
            continue;
        };
        let sender = sender.clone();
        let _ = thread::Builder::new()
            .name("xmms-control-client".to_string())
            .spawn(move || handle_client(stream, sender));
    }
}

fn handle_client(mut stream: TcpStream, sender: Sender<SocketRequest>) {
    let Ok(reader_stream) = stream.try_clone() else {
        return;
    };
    let reader = BufReader::new(reader_stream);
    for line in reader.lines() {
        let response = match line {
            Ok(line) => handle_line(line.trim(), &sender),
            Err(err) => json!({ "accepted": false, "ok": false, "error": err.to_string() }),
        };
        let response = serde_json::to_string(&response).unwrap_or_else(|_| {
            "{\"accepted\":false,\"ok\":false,\"error\":\"failed to encode ack\"}".to_string()
        });
        let _ = writeln!(stream, "{response}");
        let _ = stream.flush();
    }
}

fn handle_line(line: &str, sender: &Sender<SocketRequest>) -> Value {
    if line.is_empty() {
        return json!({ "accepted": false, "ok": false, "error": "empty command" });
    }
    let value: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(err) => return json!({ "accepted": false, "ok": false, "error": err.to_string() }),
    };
    let id = value.get("id").cloned();
    let command = match parse_socket_command(&value) {
        Ok(command) => command,
        Err(err) => return ack_json(id, SocketAck::rejected(err)),
    };
    let (reply, response) = mpsc::channel();
    if sender
        .send(SocketRequest {
            id: id.clone(),
            command,
            reply,
        })
        .is_err()
    {
        return ack_json(
            id,
            SocketAck::rejected("frontend is no longer accepting commands"),
        );
    }
    match response.recv_timeout(Duration::from_secs(5)) {
        Ok(ack) => ack_json(id, ack),
        Err(_) => ack_json(
            id,
            SocketAck::rejected("frontend did not acknowledge command"),
        ),
    }
}

fn ack_json(id: Option<Value>, ack: SocketAck) -> Value {
    let mut value = json!({ "accepted": ack.accepted, "ok": ack.accepted });
    if let Some(id) = id {
        value["id"] = id;
    }
    if let Some(error) = ack.error {
        value["error"] = Value::String(error);
    }
    value
}

pub fn parse_socket_command(value: &Value) -> Result<SocketCommand, String> {
    let command = required_str(value, "command")?;
    match normalize(command).as_str() {
        "play" => Ok(player(PlayerCommand::Play)),
        "pause" => Ok(player(PlayerCommand::Pause)),
        "togglepause" | "toggle_pause" | "playpause" | "play_pause" => {
            Ok(player(PlayerCommand::PlayPause))
        }
        "stop" | "halt" => Ok(player(PlayerCommand::Halt)),
        "previous" | "prev" | "previoustrack" | "previous_track" => {
            Ok(player(PlayerCommand::PreviousTrack))
        }
        "next" | "nexttrack" | "next_track" => Ok(player(PlayerCommand::NextTrack)),
        "seek" | "seekto" | "seek_to" => Ok(player(PlayerCommand::SeekToMs(required_i64_any(
            value,
            &["position_ms", "ms", "position"],
        )?))),
        "setvolume" | "set_volume" | "volume" => Ok(audio(AudioCommand::SetVolume(
            required_i32_any(value, &["volume", "value"])?,
        ))),
        "setbalance" | "set_balance" | "balance" => Ok(audio(AudioCommand::SetBalance(
            required_i32_any(value, &["balance", "value"])?,
        ))),
        "shuffle" | "toggleshuffle" | "toggle_shuffle" => {
            Ok(playlist(PlaylistCommand::ToggleShuffle))
        }
        "repeat" | "togglerepeat" | "toggle_repeat" => Ok(playlist(PlaylistCommand::ToggleRepeat)),
        "noadvance" | "toggle_no_advance" | "togglenoadvance" => {
            Ok(playlist(PlaylistCommand::ToggleNoAdvance))
        }
        "playlistclear" | "playlist_clear" | "clearplaylist" | "clear_playlist" => {
            Ok(playlist(PlaylistCommand::Clear))
        }
        "playlistshow" | "playlist_show" | "showplaylist" | "show_playlist" => {
            Ok(panel(PanelCommand::SetPlaylistVisibility(true)))
        }
        "playlisthide" | "playlist_hide" | "hideplaylist" | "hide_playlist" => {
            Ok(panel(PanelCommand::SetPlaylistVisibility(false)))
        }
        "playlisttoggle" | "playlist_toggle" | "toggleplaylist" | "toggle_playlist" => {
            Ok(panel(PanelCommand::TogglePlaylistVisibility))
        }
        "playlistshade" | "shadeplaylist" | "shade_playlist" => {
            Ok(panel(PanelCommand::SetPlaylistShade(true)))
        }
        "playlistunshade" | "unshadeplaylist" | "unshade_playlist" => {
            Ok(panel(PanelCommand::SetPlaylistShade(false)))
        }
        "playlistsize" | "playlist_size" => Ok(playlist(PlaylistCommand::SetSize {
            width: required_i32(value, "width")?,
            height: required_i32(value, "height")?,
        })),
        "equalizershow" | "equalizer_show" | "showequalizer" | "show_equalizer" | "eqshow" => {
            Ok(panel(PanelCommand::SetEqualizerVisibility(true)))
        }
        "equalizerhide" | "equalizer_hide" | "hideequalizer" | "hide_equalizer" | "eqhide" => {
            Ok(panel(PanelCommand::SetEqualizerVisibility(false)))
        }
        "equalizertoggle" | "equalizer_toggle" | "toggleequalizer" | "toggle_equalizer"
        | "eqtoggle" => Ok(panel(PanelCommand::ToggleEqualizerVisibility)),
        "equalizershade" | "shadeequalizer" | "shade_equalizer" => {
            Ok(panel(PanelCommand::SetEqualizerShade(true)))
        }
        "equalizerunshade" | "unshadeequalizer" | "unshade_equalizer" => {
            Ok(panel(PanelCommand::SetEqualizerShade(false)))
        }
        "mainshade" | "shade" | "shade_main" => Ok(panel(PanelCommand::SetMainShade(true))),
        "mainunshade" | "unshade" | "unshade_main" => Ok(panel(PanelCommand::SetMainShade(false))),
        "togglemainshade" | "toggle_main_shade" => Ok(panel(PanelCommand::ToggleMainShade)),
        "preferences" | "showpreferences" | "show_preferences" | "preferences_show" => Ok(
            SocketCommand::Ui(SocketUiCommand::SetPreferencesVisible(true)),
        ),
        "hidepreferences" | "hide_preferences" | "preferences_hide" => Ok(SocketCommand::Ui(
            SocketUiCommand::SetPreferencesVisible(false),
        )),
        "togglepreferences" | "toggle_preferences" => {
            Ok(SocketCommand::Ui(SocketUiCommand::TogglePreferences))
        }
        "menu" | "showmenu" | "show_menu" => {
            Ok(SocketCommand::Ui(SocketUiCommand::SetMainMenuVisible(true)))
        }
        "hidemenu" | "hide_menu" => Ok(SocketCommand::Ui(SocketUiCommand::SetMainMenuVisible(
            false,
        ))),
        "skinbrowser" | "show_skin_browser" | "skin_browser_show" => Ok(SocketCommand::Ui(
            SocketUiCommand::SetSkinBrowserVisible(true),
        )),
        "hide_skin_browser" | "skin_browser_hide" => Ok(SocketCommand::Ui(
            SocketUiCommand::SetSkinBrowserVisible(false),
        )),
        "toggle_skin_browser" => Ok(SocketCommand::Ui(SocketUiCommand::ToggleSkinBrowser)),
        "quit" | "exit" => Ok(SocketCommand::Quit),
        other => Err(format!("unknown command '{other}'")),
    }
}

fn normalize(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .replace(['-', '.', ' '], "_")
}

fn player(command: PlayerCommand) -> SocketCommand {
    SocketCommand::App(AppCommand::Player(command))
}

fn audio(command: AudioCommand) -> SocketCommand {
    SocketCommand::App(AppCommand::Audio(command))
}

fn playlist(command: PlaylistCommand) -> SocketCommand {
    SocketCommand::App(AppCommand::Playlist(command))
}

fn panel(command: PanelCommand) -> SocketCommand {
    SocketCommand::App(AppCommand::Panel(command))
}

fn required_str<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing string field '{key}'"))
}

fn required_i32(value: &Value, key: &str) -> Result<i32, String> {
    let number = value
        .get(key)
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("missing integer field '{key}'"))?;
    i32::try_from(number).map_err(|_| format!("field '{key}' is out of range"))
}

fn required_i32_any(value: &Value, keys: &[&str]) -> Result<i32, String> {
    for key in keys {
        if value.get(key).is_some() {
            return required_i32(value, key);
        }
    }
    Err(format!(
        "missing integer field '{}'; accepted: {}",
        keys[0],
        keys.join(", ")
    ))
}

fn required_i64_any(value: &Value, keys: &[&str]) -> Result<i64, String> {
    for key in keys {
        if let Some(number) = value.get(key).and_then(Value::as_i64) {
            return Ok(number);
        }
    }
    Err(format!(
        "missing integer field '{}'; accepted: {}",
        keys[0],
        keys.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_panel_visibility_command() {
        let value = json!({ "command": "show_playlist" });
        assert_eq!(
            parse_socket_command(&value).unwrap(),
            SocketCommand::App(AppCommand::Panel(PanelCommand::SetPlaylistVisibility(true)))
        );
    }

    #[test]
    fn parses_volume_command() {
        let value = json!({ "command": "set_volume", "volume": 42 });
        assert_eq!(
            parse_socket_command(&value).unwrap(),
            SocketCommand::App(AppCommand::Audio(AudioCommand::SetVolume(42)))
        );
    }
}
