//documentation
#![doc = include_str!("../README.md")]
#[allow(unused_imports)]
use crate as bevy_worldswap;

//module tree
mod app;
mod plugins;
mod run_conditions;
mod subapp;
mod window_utils;

//API exports
pub(crate) use crate::subapp::*;
pub(crate) use crate::window_utils::*;
pub(crate) use crate::prelude::*;

pub mod prelude
{
    pub use crate::app::*;
    pub use crate::plugins::*;
    pub use crate::run_conditions::*;
}
