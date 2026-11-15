//! Shared fixtures for mod.

use pyreal_client_net::client_session::testing::MockTransport;
use pyreal_client_net::client_session::{PositionReporter, Session};
use pyreal_primitives::LocalTime;
use pyreal_protocol::login::{
    CharGenVerificationResponse, CharacterIdentity, LoginPlayerDescription, PlayerModule,
};

pub(crate) fn t(s: f64) -> LocalTime {
    LocalTime(s)
}

pub(crate) fn session() -> Session<MockTransport> {
    Session::new(MockTransport::new())
}

pub(crate) fn player_description() -> LoginPlayerDescription {
    LoginPlayerDescription {
        player_module: PlayerModule {
            spell_bars: vec![Vec::new()],
            spell_filters: PlayerModule::DEFAULT_SPELL_FILTERS,
            options2: PlayerModule::DEFAULT_OPTIONS2,
            ..PlayerModule::default()
        },
        ..LoginPlayerDescription::default()
    }
}

pub(crate) fn reporter() -> (PositionReporter, Session<MockTransport>) {
    let mut r = PositionReporter::new(0.0);
    r.active = true;
    (r, Session::new(MockTransport::new()))
}

pub(crate) fn response(code: u32, identity: CharacterIdentity) -> CharGenVerificationResponse {
    CharGenVerificationResponse {
        response_type: code,
        identity,
    }
}

pub(crate) mod session_fixture;

pub(crate) mod attack_notifications;
