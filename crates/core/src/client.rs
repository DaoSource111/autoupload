//! Which YouTube a channel talks to: the real API with its saved sign-in, or the offline demo.

use crate::auth::Session;
use crate::channel::Channel;
use crate::demo::{self, DemoApi};
use crate::youtube::{Api, YouTube};

/// The API client for a channel. In demo mode it is the offline [`DemoApi`]; otherwise the channel must be signed in.
pub fn api_for(ch: &Channel) -> Result<Box<dyn Api>, String> {
    if demo::active() {
        return Ok(Box::new(DemoApi::for_channel(ch)));
    }
    Session::load(&ch.profile)
        .map(|s| Box::new(YouTube::new(s)) as Box<dyn Api>)
        .map_err(|e| e.0)
}
