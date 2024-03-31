use bevy::app::SubApp;
use bevy::ecs::schedule::ScheduleLabel;
use bevy::prelude::*;
use bevy::render::pipelined_rendering::RenderExtractApp;
use bevy::render::RenderApp;
use bevy::time::{TimeReceiver, TimeSender};

use crate::*;

//-------------------------------------------------------------------------------------------------------------------

/// Command that can be sent with [`SwapCommandSender`] to control which world is running.
///
/// Swap commands provide a simple 1-layer 'fork-join' pattern. Use [`Fork`](SwapCommand::Fork) in the initial
/// world to put it in the background and run another world in the foreground. Use [`Pass`](SwapCommand::Pass) to
/// drop the foreground world and run another world in the foreground. Use [`Join`](SwapCommand::Join) to drop the
/// foreground world and put the background world in the foreground.
///
/// Both the foreground and background worlds can send [`Pass`](SwapCommand::Pass), [`Swap`](SwapCommand::Swap),
/// and [`Join`](SwapCommand::Join) commands. Only foreground worlds can send [`Fork`](SwapCommand::Fork), and only
/// if there is no background world.
///
/// Note that when a world is dropped due to [`Pass`](SwapCommand::Pass) or [`Join`](SwapCommand::Join), an
/// `AppExit` event will not be sent to that world unless the world generated the event itself.
pub enum SwapCommand
{
    /// Swap in another app's world and drop the current world.
    Pass(WorldSwapApp),
    /// Swap in another app's world and put the current world in the background.
    ///
    /// # Panics
    ///
    /// Panics if there is already a world in the background.
    Fork(WorldSwapApp),
    /// Swap in the background world and put the current world in the background.
    ///
    /// # Panics
    ///
    /// Panics if there is no world in the background.
    Swap,
    /// Swap in the background world and drop the current world.
    ///
    /// Note that if the background world sent `AppExit` at any point in the past, then as soon as it enters the
    /// foreground the app will shut down.
    ///
    /// # Panics
    ///
    /// Panics if there is no world in the background.
    Join,
}

//-------------------------------------------------------------------------------------------------------------------

/// Resource for sending [`SwapCommands`](SwapCommand).
///
/// Only the last swap command sent during a tick will be applied. If a foreground and background world send
/// commands in the same tick, then the foreground command will take precedence.
#[derive(Resource, Clone)]
pub struct SwapCommandSender(pub(crate) crossbeam::channel::Sender<SwapCommand>);

impl SwapCommandSender
{
    /// Sends a [`SwapCommand`] to the `bevy_worldswap` backend.
    pub fn send(&self, command: SwapCommand)
    {
        // Ignore errors.
        let _ = self.0.send(command);
    }
}

//-------------------------------------------------------------------------------------------------------------------

/// Resource for receiving [`SwapCommands`](SwapCommand).
///
/// Only used in [`WorldSwapSubApp`].
#[derive(Resource, Deref)]
pub(crate) struct SwapCommandReceiver(pub(crate) crossbeam::channel::Receiver<SwapCommand>);

//-------------------------------------------------------------------------------------------------------------------

/// Resource that records the world-swap status of a world.
///
/// This is controlled by the `bevy_worldswap` backend.
#[derive(Resource, Copy, Clone, Eq, PartialEq)]
pub enum WorldSwapStatus
{
    /// The world is suspended.
    Suspended,
    /// The world is running in the foreground.
    Foreground,
    /// The world is running in the background.
    ///
    /// Note that the background world may not update if [`BackgroundTickRate::Never`] is configured in
    /// [`WorldSwapPlugin`].
    Background,
}

//-------------------------------------------------------------------------------------------------------------------

/// Stores a [`World`] that is not in the foreground.
///
/// The world might be [`Suspended`](WorldSwapStatus::Suspended) or in the
/// [`Background`](WorldSwapStatus::Background).
//todo: configure with bevy_render flag
pub struct WorldSwapApp
{
    /// The stored world.
    pub world: World,
    /// This world's tick policy when it is in the background.
    ///
    /// If `None` then the default tick rate configured in [`WorldSwapPlugin`] will be used.
    pub background_tick_rate: Option<BackgroundTickRate>,
    /// Indicates if the world was paused due to BackgroundTickRate::Never::freeze_time.
    ///
    /// If this is true, then the world will be unpaused when swapped into the foreground.
    pub(crate) paused_by_tick_policy: bool,
    /// Receives time from this world's [`RenderApp`].
    ///
    /// Cached while the world is away from the foreground so its internal time will increment properly. Normally,
    /// worlds that render will have their time sent from [`RenderApp`].
    pub(crate) time_receiver: Option<TimeReceiver>,
    /// Sends time to this world.
    ///
    /// Cached so that time can be sent while in the foreground when not rendering while waiting for the previous
    /// world to finish rendering.
    pub(crate) time_sender: Option<TimeSender>,
    /// The world's [`RenderApp`] or [`RenderExtractApp`].
    ///
    /// Cached while the world is away from the foreground.
    pub(crate) render_app: Option<SubApp>,
}

impl WorldSwapApp
{
    /// Creates a new world-swap wrapper for a fresh [`App`].
    ///
    /// This method calls [`App::finish`] and [`App::cleanup`] on the app before removing its contents.
    ///
    /// The app will have the default background tick rate configured in [`WorldSwapPlugin`]. Use
    /// [`Self::new_with`] if you want a specific tick rate for this app.
    ///
    /// ## Panics
    /// - If the app's [`main_schedule_label`](App::main_schedule_label) is not [`Main`].
    pub fn new(mut app: App) -> Self
    {
        if app.main().update_schedule != Some(Main.intern()) {
            panic!("failed making WorldSwapApp, app's main_schedule_label is not Main");
        }
        app.insert_resource(WorldSwapStatus::Suspended);
        app.finish();
        app.cleanup();
        let time_receiver = app.world_mut().remove_resource::<TimeReceiver>();
        let time_sender = app.world_mut().remove_resource::<TimeSender>();
        let render_app = app
            .remove_sub_app(RenderApp)
            .or_else(|| app.remove_sub_app(RenderExtractApp));
        Self {
            world: std::mem::take(app.world_mut()),
            background_tick_rate: None,
            paused_by_tick_policy: false,
            time_receiver,
            time_sender,
            render_app,
        }
    }

    /// Creates a new world-swap wrapper for a fresh [`App`] with a specific [`BackgroundTickRate`].
    ///
    /// See [`Self::new`].
    pub fn new_with(app: App, background_tick_rate: BackgroundTickRate) -> Self
    {
        let mut app = Self::new(app);
        app.background_tick_rate = Some(background_tick_rate);
        app
    }
}

//-------------------------------------------------------------------------------------------------------------------
