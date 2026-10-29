//! Changing the greeter's settings as the user: the settings file and the
//! wallpaper folder are root's, and change in one run of `gpui-greeter
//! apply-settings` under pkexec, which takes a closed set of changes and
//! checks every one (see [`helper`]).

pub mod helper;
pub mod request;
pub mod setting;
