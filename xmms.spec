%global app_id org.xmms.XMMS

Name:           xmms
Version:        2.0.0
Release:        1%{?dist}
Summary:        Music player with Winamp-compatible skinning

License:        GPL-2.0-or-later
URL:            https://github.com/xmms
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  meson >= 0.59
BuildRequires:  gcc
BuildRequires:  pkgconfig(gtk4) >= 4.6
BuildRequires:  pkgconfig(gstreamer-1.0) >= 1.16
BuildRequires:  pkgconfig(libarchive) >= 3.0
BuildRequires:  pkgconfig(libsoup-3.0) >= 3.0
BuildRequires:  pkgconfig(json-glib-1.0) >= 1.6
BuildRequires:  desktop-file-utils
BuildRequires:  libappstream-glib

Requires:       gtk4%{?_isa}
Requires:       gstreamer1%{?_isa}
Requires:       gstreamer1-plugins-base%{?_isa}
Requires:       gstreamer1-plugins-good%{?_isa}

Recommends:     gstreamer1-plugins-bad-free%{?_isa}
Recommends:     gstreamer1-plugins-ugly-free%{?_isa}

%description
XMMS (X Multimedia System) is a music player that supports Winamp-compatible
skins for a fully customizable user interface. It uses GStreamer for audio
playback, supporting a wide range of audio formats including MP3, OGG, FLAC,
WAV, and more.

Features include a 10-band equalizer, spectrum analyzer visualization,
playlist management, MPRIS2 media key integration, and Spotify playlist
browsing.

%prep
%autosetup

%build
%meson
%meson_build

%install
%meson_install

%check
desktop-file-validate %{buildroot}%{_datadir}/applications/%{app_id}.desktop
appstream-util validate-relax --nonet %{buildroot}%{_datadir}/metainfo/%{app_id}.metainfo.xml

%files
%license COPYING
%doc AUTHORS
%{_bindir}/xmms
%{_mandir}/man1/xmms.1*
%{_datadir}/applications/%{app_id}.desktop
%{_datadir}/metainfo/%{app_id}.metainfo.xml

%changelog
* Sat Mar 21 2026 XMMS Contributors <xmms@xmms.org> - 2.0.0-1
- Modernized release using GTK 4 and GStreamer
- Winamp-compatible skin support with Cairo rendering
- 10-band equalizer with spectrum analyzer visualization
- MPRIS2 D-Bus interface for media key integration
- Spotify playlist browsing via Web API
- Skin browser for installed skins
