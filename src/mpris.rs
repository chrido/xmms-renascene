use crate::player::PlayerState;

pub const BUS_NAME: &str = "org.mpris.MediaPlayer2.xmms_renascene";
pub const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
pub const ROOT_INTERFACE: &str = "org.mpris.MediaPlayer2";
pub const PLAYER_INTERFACE: &str = "org.mpris.MediaPlayer2.Player";
pub const DBUS_PROPERTIES_INTERFACE: &str = "org.freedesktop.DBus.Properties";

pub const INTROSPECTION_XML: &str = r#"
<node>
  <interface name='org.mpris.MediaPlayer2'>
    <method name='Raise'/>
    <method name='Quit'/>
    <property name='CanQuit' type='b' access='read'/>
    <property name='CanRaise' type='b' access='read'/>
    <property name='HasTrackList' type='b' access='read'/>
    <property name='Identity' type='s' access='read'/>
    <property name='DesktopEntry' type='s' access='read'/>
    <property name='SupportedUriSchemes' type='as' access='read'/>
    <property name='SupportedMimeTypes' type='as' access='read'/>
  </interface>
  <interface name='org.mpris.MediaPlayer2.Player'>
    <method name='Next'/>
    <method name='Previous'/>
    <method name='Pause'/>
    <method name='PlayPause'/>
    <method name='Stop'/>
    <method name='Play'/>
    <method name='Seek'>
      <arg name='Offset' type='x' direction='in'/>
    </method>
    <method name='SetPosition'>
      <arg name='TrackId' type='o' direction='in'/>
      <arg name='Position' type='x' direction='in'/>
    </method>
    <method name='OpenUri'>
      <arg name='Uri' type='s' direction='in'/>
    </method>
    <signal name='Seeked'>
      <arg name='Position' type='x'/>
    </signal>
    <property name='PlaybackStatus' type='s' access='read'/>
    <property name='Rate' type='d' access='read'/>
    <property name='Metadata' type='a{sv}' access='read'/>
    <property name='Volume' type='d' access='readwrite'/>
    <property name='Position' type='x' access='read'/>
    <property name='CanGoNext' type='b' access='read'/>
    <property name='CanGoPrevious' type='b' access='read'/>
    <property name='CanPlay' type='b' access='read'/>
    <property name='CanPause' type='b' access='read'/>
    <property name='CanSeek' type='b' access='read'/>
    <property name='CanControl' type='b' access='read'/>
  </interface>
</node>
"#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MprisRootProperties {
    pub can_quit: bool,
    pub can_raise: bool,
    pub has_track_list: bool,
    pub identity: &'static str,
    pub desktop_entry: &'static str,
    pub supported_uri_schemes: Vec<&'static str>,
    pub supported_mime_types: Vec<&'static str>,
}

impl Default for MprisRootProperties {
    fn default() -> Self {
        Self {
            can_quit: true,
            can_raise: true,
            has_track_list: false,
            identity: "XMMS Renascene",
            desktop_entry: "org.xmms.Renascene",
            supported_uri_schemes: vec!["file", "http", "https"],
            supported_mime_types: vec![
                "audio/mpeg",
                "audio/ogg",
                "audio/flac",
                "audio/x-wav",
                "audio/mp4",
            ],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MprisMetadata {
    pub track_id: String,
    pub title: Option<String>,
    pub url: Option<String>,
    pub length_us: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MprisPlayerProperties {
    pub playback_status: &'static str,
    pub rate: f64,
    pub metadata: MprisMetadata,
    pub volume: f64,
    pub position_us: i64,
    pub can_go_next: bool,
    pub can_go_previous: bool,
    pub can_play: bool,
    pub can_pause: bool,
    pub can_seek: bool,
    pub can_control: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MprisCommand {
    Raise,
    Quit,
    Next,
    Previous,
    Pause,
    PlayPause,
    Stop,
    Play,
    Seek { offset_us: i64 },
    SetPosition { track_id: String, position_us: i64 },
    OpenUri(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MprisEvent {
    Raised,
    QuitRequested,
    MetadataChanged,
    PlaybackStatusChanged,
    Seeked(i64),
}

pub fn playback_status(state: PlayerState) -> &'static str {
    match state {
        PlayerState::Playing => "Playing",
        PlayerState::Paused => "Paused",
        PlayerState::Stopped => "Stopped",
    }
}

pub fn command_for_method(
    method_name: &str,
    parameters: &MprisMethodParameters,
) -> Option<MprisCommand> {
    match method_name {
        "Raise" => Some(MprisCommand::Raise),
        "Quit" => Some(MprisCommand::Quit),
        "Next" => Some(MprisCommand::Next),
        "Previous" => Some(MprisCommand::Previous),
        "Pause" => Some(MprisCommand::Pause),
        "PlayPause" => Some(MprisCommand::PlayPause),
        "Stop" => Some(MprisCommand::Stop),
        "Play" => Some(MprisCommand::Play),
        "Seek" => parameters
            .seek_offset_us
            .map(|offset_us| MprisCommand::Seek { offset_us }),
        "SetPosition" => parameters
            .set_position
            .as_ref()
            .map(|(track_id, position_us)| MprisCommand::SetPosition {
                track_id: track_id.clone(),
                position_us: *position_us,
            }),
        "OpenUri" => parameters
            .open_uri
            .as_ref()
            .map(|uri| MprisCommand::OpenUri(uri.clone())),
        _ => None,
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MprisMethodParameters {
    pub seek_offset_us: Option<i64>,
    pub set_position: Option<(String, i64)>,
    pub open_uri: Option<String>,
}

pub mod gio_service {
    use std::cell::RefCell;
    use std::rc::Rc;

    use gtk::gio::{
        self, BusNameOwnerFlags, BusType, DBusConnection, DBusInterfaceInfo, DBusNodeInfo, OwnerId,
        RegistrationId,
    };
    use gtk::glib::{self, variant::ToVariant, Variant, VariantDict};

    use super::{
        command_for_method, MprisEvent, MprisMetadata, MprisMethodParameters,
        MprisPlayerProperties, MprisRootProperties, BUS_NAME, DBUS_PROPERTIES_INTERFACE,
        INTROSPECTION_XML, OBJECT_PATH, PLAYER_INTERFACE, ROOT_INTERFACE,
    };
    use crate::ui::MainWindowUiState;

    pub struct MprisService {
        owner_id: Option<OwnerId>,
        connection: Rc<RefCell<Option<DBusConnection>>>,
        registrations: Rc<RefCell<Vec<RegistrationId>>>,
    }

    impl MprisService {
        pub(crate) fn own_session_bus(state: Rc<RefCell<MainWindowUiState>>) -> Self {
            let connection = Rc::new(RefCell::new(None));
            let registrations = Rc::new(RefCell::new(Vec::new()));
            let bus_connection = Rc::clone(&connection);
            let bus_registrations = Rc::clone(&registrations);

            let owner_id = gio::bus_own_name(
                BusType::Session,
                BUS_NAME,
                BusNameOwnerFlags::empty(),
                move |connection, _name| {
                    if let Err(err) = register_mpris_object(
                        &connection,
                        Rc::clone(&state),
                        Rc::clone(&bus_registrations),
                    ) {
                        eprintln!("xmms-rs: failed to register MPRIS object: {err}");
                    }
                    *bus_connection.borrow_mut() = Some(connection);
                },
                |_connection, _name| {},
                {
                    let connection = Rc::clone(&connection);
                    let registrations = Rc::clone(&registrations);
                    move |_connection, _name| {
                        registrations.borrow_mut().clear();
                        *connection.borrow_mut() = None;
                    }
                },
            );

            Self {
                owner_id: Some(owner_id),
                connection,
                registrations,
            }
        }

        pub fn emit_events(&self, events: &[MprisEvent], properties: &MprisPlayerProperties) {
            let Some(connection) = self.connection.borrow().as_ref().cloned() else {
                return;
            };

            for event in events {
                match event {
                    MprisEvent::Seeked(position_us) => {
                        let _ = connection.emit_signal(
                            None,
                            OBJECT_PATH,
                            PLAYER_INTERFACE,
                            "Seeked",
                            Some(&(*position_us,).to_variant()),
                        );
                    }
                    MprisEvent::MetadataChanged | MprisEvent::PlaybackStatusChanged => {
                        let changed = player_properties_changed_variant(properties);
                        let _ = connection.emit_signal(
                            None,
                            OBJECT_PATH,
                            DBUS_PROPERTIES_INTERFACE,
                            "PropertiesChanged",
                            Some(&(PLAYER_INTERFACE, changed, Vec::<String>::new()).to_variant()),
                        );
                    }
                    MprisEvent::Raised | MprisEvent::QuitRequested => {}
                }
            }
        }
    }

    impl Drop for MprisService {
        fn drop(&mut self) {
            if let Some(connection) = self.connection.borrow().as_ref() {
                for registration in self.registrations.borrow_mut().drain(..) {
                    let _ = connection.unregister_object(registration);
                }
            }
            if let Some(owner_id) = self.owner_id.take() {
                gio::bus_unown_name(owner_id);
            }
        }
    }

    pub fn introspection_interfaces() -> Result<Vec<String>, glib::Error> {
        let node = DBusNodeInfo::for_xml(INTROSPECTION_XML)?;
        Ok(node
            .interfaces()
            .iter()
            .map(|interface| interface.name().to_string())
            .collect())
    }

    fn register_mpris_object(
        connection: &DBusConnection,
        state: Rc<RefCell<MainWindowUiState>>,
        registrations: Rc<RefCell<Vec<RegistrationId>>>,
    ) -> Result<(), glib::Error> {
        let node = DBusNodeInfo::for_xml(INTROSPECTION_XML)?;
        let root = node
            .lookup_interface(ROOT_INTERFACE)
            .expect("MPRIS root interface must be present");
        let player = node
            .lookup_interface(PLAYER_INTERFACE)
            .expect("MPRIS player interface must be present");

        let root_id = register_interface(connection, &root, Rc::clone(&state))?;
        let player_id = register_interface(connection, &player, state)?;
        registrations.borrow_mut().extend([root_id, player_id]);
        Ok(())
    }

    fn register_interface(
        connection: &DBusConnection,
        interface: &DBusInterfaceInfo,
        state: Rc<RefCell<MainWindowUiState>>,
    ) -> Result<RegistrationId, glib::Error> {
        let method_state = Rc::clone(&state);
        let get_state = Rc::clone(&state);
        let set_state = state;

        connection
            .register_object(OBJECT_PATH, interface)
            .method_call(
                move |_connection,
                      _sender,
                      _object_path,
                      interface_name,
                      method_name,
                      parameters,
                      invocation| {
                    let parsed = parameters_from_variant(method_name, &parameters);
                    if let Some(command) = command_for_method(method_name, &parsed) {
                        method_state.borrow_mut().execute_mpris_command(command);
                        invocation.return_value(None);
                    } else {
                        invocation.return_dbus_error(
                            "org.mpris.MediaPlayer2.Error.NotSupported",
                            &format!(
                                "Unsupported MPRIS method {}.{}",
                                interface_name.unwrap_or(""),
                                method_name
                            ),
                        );
                    }
                },
            )
            .property(
                move |_connection, _sender, _object_path, interface_name, property_name| {
                    property_variant(&get_state.borrow(), interface_name, property_name)
                },
            )
            .set_property(
                move |_connection, _sender, _object_path, interface_name, property_name, value| {
                    if interface_name == PLAYER_INTERFACE && property_name == "Volume" {
                        if let Ok(volume) = value.try_get::<f64>() {
                            set_state.borrow_mut().set_mpris_volume(volume);
                            return true;
                        }
                    }
                    false
                },
            )
            .build()
    }

    fn parameters_from_variant(method_name: &str, parameters: &Variant) -> MprisMethodParameters {
        match method_name {
            "Seek" => MprisMethodParameters {
                seek_offset_us: Some(parameters.child_get::<i64>(0)),
                ..MprisMethodParameters::default()
            },
            "SetPosition" => MprisMethodParameters {
                set_position: Some((
                    parameters.child_get::<String>(0),
                    parameters.child_get::<i64>(1),
                )),
                ..MprisMethodParameters::default()
            },
            "OpenUri" => MprisMethodParameters {
                open_uri: Some(parameters.child_get::<String>(0)),
                ..MprisMethodParameters::default()
            },
            _ => MprisMethodParameters::default(),
        }
    }

    fn property_variant(
        state: &MainWindowUiState,
        interface_name: &str,
        property_name: &str,
    ) -> Variant {
        match interface_name {
            ROOT_INTERFACE => root_property_variant(&state.mpris_root_properties(), property_name),
            PLAYER_INTERFACE => {
                player_property_variant(&state.mpris_player_properties(), property_name)
            }
            _ => false.to_variant(),
        }
    }

    fn root_property_variant(properties: &MprisRootProperties, property_name: &str) -> Variant {
        match property_name {
            "CanQuit" => properties.can_quit.to_variant(),
            "CanRaise" => properties.can_raise.to_variant(),
            "HasTrackList" => properties.has_track_list.to_variant(),
            "Identity" => properties.identity.to_variant(),
            "DesktopEntry" => properties.desktop_entry.to_variant(),
            "SupportedUriSchemes" => properties.supported_uri_schemes.to_variant(),
            "SupportedMimeTypes" => properties.supported_mime_types.to_variant(),
            _ => false.to_variant(),
        }
    }

    fn player_property_variant(properties: &MprisPlayerProperties, property_name: &str) -> Variant {
        match property_name {
            "PlaybackStatus" => properties.playback_status.to_variant(),
            "Rate" => properties.rate.to_variant(),
            "Metadata" => metadata_variant(&properties.metadata),
            "Volume" => properties.volume.to_variant(),
            "Position" => properties.position_us.to_variant(),
            "CanGoNext" => properties.can_go_next.to_variant(),
            "CanGoPrevious" => properties.can_go_previous.to_variant(),
            "CanPlay" => properties.can_play.to_variant(),
            "CanPause" => properties.can_pause.to_variant(),
            "CanSeek" => properties.can_seek.to_variant(),
            "CanControl" => properties.can_control.to_variant(),
            _ => false.to_variant(),
        }
    }

    fn metadata_variant(metadata: &MprisMetadata) -> Variant {
        let dict = VariantDict::new(None);
        dict.insert("mpris:trackid", metadata.track_id.as_str());
        if let Some(title) = metadata.title.as_deref() {
            dict.insert("xesam:title", title);
        }
        if let Some(url) = metadata.url.as_deref() {
            dict.insert("xesam:url", url);
        }
        if let Some(length_us) = metadata.length_us {
            dict.insert("mpris:length", length_us);
        }
        dict.end()
    }

    fn player_properties_changed_variant(properties: &MprisPlayerProperties) -> Variant {
        let dict = VariantDict::new(None);
        dict.insert("PlaybackStatus", properties.playback_status);
        dict.insert_value("Metadata", &metadata_variant(&properties.metadata));
        dict.insert("Volume", properties.volume);
        dict.insert("Position", properties.position_us);
        dict.end()
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::mpris::MprisCommand;
        use gtk::glib::VariantTy;

        #[test]
        fn introspection_xml_exposes_root_and_player_interfaces() {
            let interfaces = introspection_interfaces().unwrap();

            assert!(interfaces.contains(&ROOT_INTERFACE.to_string()));
            assert!(interfaces.contains(&PLAYER_INTERFACE.to_string()));
        }

        #[test]
        fn metadata_variant_uses_mpris_xesam_keys() {
            let metadata = MprisMetadata {
                track_id: "/org/xmms/Track/1".to_string(),
                title: Some("Track Title".to_string()),
                url: Some("file:///tmp/track.ogg".to_string()),
                length_us: Some(12_000_000),
            };
            let variant = metadata_variant(&metadata);
            let dict = VariantDict::new(Some(&variant));

            assert_eq!(
                dict.lookup::<String>("mpris:trackid").unwrap().as_deref(),
                Some("/org/xmms/Track/1")
            );
            assert_eq!(
                dict.lookup::<String>("xesam:title").unwrap().as_deref(),
                Some("Track Title")
            );
            assert_eq!(
                dict.lookup::<String>("xesam:url").unwrap().as_deref(),
                Some("file:///tmp/track.ogg")
            );
            assert_eq!(
                dict.lookup::<i64>("mpris:length").unwrap(),
                Some(12_000_000)
            );
            assert_eq!(variant.type_(), VariantTy::new("a{sv}").unwrap());
        }

        #[test]
        fn method_names_map_to_deterministic_commands() {
            assert_eq!(
                command_for_method(
                    "Seek",
                    &MprisMethodParameters {
                        seek_offset_us: Some(42),
                        ..MprisMethodParameters::default()
                    },
                ),
                Some(MprisCommand::Seek { offset_us: 42 })
            );
            assert_eq!(
                command_for_method(
                    "OpenUri",
                    &MprisMethodParameters {
                        open_uri: Some("file:///tmp/a.ogg".to_string()),
                        ..MprisMethodParameters::default()
                    },
                ),
                Some(MprisCommand::OpenUri("file:///tmp/a.ogg".to_string()))
            );
            assert_eq!(
                command_for_method("Unknown", &MprisMethodParameters::default()),
                None
            );
        }
    }
}
