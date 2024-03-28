use std::sync::{Arc, Mutex};

use bevy::a11y::Focus;
use bevy::app::{PluginGroupBuilder, SubApp};
use bevy::ecs::schedule::ScheduleLabel;
use bevy::prelude::*;
use bevy::render::pipelined_rendering::RenderExtractApp;
use bevy::render::renderer::{RenderAdapter, RenderAdapterInfo, RenderDevice, RenderInstance, RenderQueue};
use bevy::render::settings::RenderCreation;
use bevy::render::{RenderApp, RenderPlugin};
use bevy::window::{
    ExitCondition, PrimaryWindow, WindowBackendScaleFactorChanged, WindowScaleFactorChanged, WindowThemeChanged,
};
use bevy::winit::{WinitCorePlugin, WinitPlugin};

use crate::*;

//-------------------------------------------------------------------------------------------------------------------
//-------------------------------------------------------------------------------------------------------------------

fn collect_window_events(
    windows: Query<(), With<Window>>,
    mut removed_windows: RemovedComponents<Window>,
    mut backend_scale_factor_events: EventReader<WindowBackendScaleFactorChanged>,
    mut scale_factor_events: EventReader<WindowScaleFactorChanged>,
    mut theme_events: EventReader<WindowThemeChanged>,
    mut event_cache: ResMut<WindowEventCache>,
)
{
    // Clean up existing entries to avoid memory leak for spawing/despawning windows.
    for removed in removed_windows.read() {
        if windows.contains(removed) {
            continue;
        }
        event_cache.remove(removed);
    }

    // Collect events.
    for event in backend_scale_factor_events.read() {
        if !windows.contains(event.window) {
            continue;
        }
        event_cache.insert_backend_scale_factor_event(event.clone());
    }

    for event in scale_factor_events.read() {
        if !windows.contains(event.window) {
            continue;
        }
        event_cache.insert_scale_factor_event(event.clone());
    }

    for event in theme_events.read() {
        if !windows.contains(event.window) {
            continue;
        }
        event_cache.insert_theme_event(event.clone());
    }
}

//-------------------------------------------------------------------------------------------------------------------
//-------------------------------------------------------------------------------------------------------------------

/// Plugin for inserting an asset server as a resource.
///
/// Used in ChildDefaultPlugins.
struct InsertAssetServerPlugin
{
    asset_server: Arc<Mutex<Option<AssetServer>>>,
}

impl InsertAssetServerPlugin
{
    fn new(asset_server: AssetServer) -> Self
    {
        Self { asset_server: Arc::new(Mutex::new(Some(asset_server))) }
    }
}

impl Plugin for InsertAssetServerPlugin
{
    fn build(&self, app: &mut App)
    {
        app.insert_resource(self.asset_server.lock().unwrap().take().unwrap());
    }
}

//-------------------------------------------------------------------------------------------------------------------
//-------------------------------------------------------------------------------------------------------------------

/// Plugin to use in addition to [`WindowPlugin`] for child worlds.
///
/// We need to manually repair the `Focus` resource since the primary window isn't spawned by `WindowPlugin` for
/// child worlds.
struct ChildFocusRepairPlugin;

impl Plugin for ChildFocusRepairPlugin
{
    fn build(&self, app: &mut App)
    {
        app.add_systems(
            PreStartup,
            |mut focus: ResMut<Focus>, primary: Query<Entity, (With<Window>, With<PrimaryWindow>)>| {
                let Ok(primary) = primary.get_single() else { return };
                **focus = Some(primary);
            },
        );
    }
}

//-------------------------------------------------------------------------------------------------------------------
//-------------------------------------------------------------------------------------------------------------------

struct WorldSwapWindowPlugin;

impl Plugin for WorldSwapWindowPlugin
{
    fn build(&self, app: &mut App)
    {
        app.init_resource::<WindowEventCache>()
            .add_event::<WindowBackendScaleFactorChanged>()
            .add_event::<WindowScaleFactorChanged>()
            .add_event::<WindowThemeChanged>()
            .add_systems(Last, collect_window_events.in_set(WorldSwapSet));
    }
}

//-------------------------------------------------------------------------------------------------------------------
//-------------------------------------------------------------------------------------------------------------------

/// System set that runs in [`Last`].
///
/// Window events are collected in this set.
#[derive(SystemSet, Default, Debug, Eq, PartialEq, Hash, Copy, Clone)]
pub struct WorldSwapSet;

//-------------------------------------------------------------------------------------------------------------------

/// If you want to reuse the parent world's assets in the child world, then you must insert a clone of the parent
/// world's [`AssetServer`] to the child world. This should be done before adding [`AssetPlugin`] to your app,
/// otherwise an extra asset server will be constructed and dropped needlessly.

//-------------------------------------------------------------------------------------------------------------------

/// Controls how a background world will update.
#[derive(Debug, Copy, Clone)]
pub enum BackgroundTickRate
{
    /// The background world never updates.
    ///
    /// If `freeze_time` is true then the background world's virtual time will be frozen while in the background.
    ///
    /// If you manually pause a world's virtual time with [`Time::pause`] before sending it to the background,
    /// then this option will have no effect. The world will still be paused when it re-enters the foreground.
    Never
    {
        freeze_time: bool
    },
    /// The background world updates in every tick that the main world updates.
    EveryTick,
    // /// The background world updates at a fixed tick rate.
    // ///
    // /// The background world won't update more than once per main world tick.
    //todo: TickRate,
    // /// The background world will update once in each main world tick where this callback returns true.
    //todo: Custom(callback fn),
}

//-------------------------------------------------------------------------------------------------------------------

pub type SwapRecoveryFn = fn(&mut World, WorldSwapApp);

//-------------------------------------------------------------------------------------------------------------------

/// Sets up world swapping for an [`App`].
///
/// Don't use this for setting up secondary apps. There are two types of secondary apps, headless and windowed.
/// - **Headless**: No extra plugin is required. If your secondary app will load assets, clone the parent's
/// [`AssetServer`] resource into the app (insert it *before* [`AssetPlugin`]).
/// - **Windowed**: Use [`ChildDefaultPlugins`] instead of [`DefaultPlugins`].
///
/// # Panics
/// - Panics if the app's [`App::main_schedule_label`] is not [`Main`].
/// - Panics if the `bevy/bevy_render` feature is enabled but this plugin isn't added after [`DefaultPlugins`].
#[derive(Resource, Clone)]
pub struct WorldSwapPlugin
{
    /// Controls how background worlds update while in the background.
    ///
    /// Can be overridden when creating child worlds with [`WorldSwapApp::new_with`].
    ///
    /// The world in the initial app will be assigned this background tick rate when it moves to the background.
    ///
    /// By default, equals [`BackgroundTickRate::Never`] with `freeze_time = true`.
    pub background_tick_rate: BackgroundTickRate,
    /// Callback called when a [`SwapCommand::Pass`] is applied.
    ///
    /// This allows you to pass data from the passing world to the new world, or even cache the [`WorldSwapApp`]
    /// and resume it later with [`SwapCommand::Fork`] or [`SwapCommand::Pass`].
    pub swap_pass_recovery: Option<SwapRecoveryFn>,
    /// Callback called when a [`SwapCommand::Join`] is applied.
    ///
    /// This allows you to pass data from the joining world to the background world, or even cache the
    /// [`WorldSwapApp`] and resume it later with [`SwapCommand::Fork`] or [`SwapCommand::Pass`].
    ///
    /// Note that time in the world in a [`WorldSwapApp`] passed to [`SwapRecoveryFn`] will *not* be paused unless
    /// you manually pause it. The `freeze_time` option in [`BackgroundTickRate::Never`] only applies to worlds in
    /// the background.
    pub swap_join_recovery: Option<SwapRecoveryFn>,
    /// Controls whether then app should shut down when the background world exits.
    ///
    /// This does nothing on [`BackgroundTickRate::Never`].
    ///
    /// False by default.
    pub abort_on_background_exit: bool,
}

impl Default for WorldSwapPlugin
{
    fn default() -> Self
    {
        Self {
            background_tick_rate: BackgroundTickRate::Never { freeze_time: true },
            swap_pass_recovery: None,
            swap_join_recovery: None,
            abort_on_background_exit: false,
        }
    }
}

impl Plugin for WorldSwapPlugin
{
    fn build(&self, app: &mut App)
    {
        // Require app uses the `Main` schedule, in order to ensure consistency between the initial app and child
        // apps.
        if app.main_schedule_label != Main.intern() {
            panic!("failed adding WorldSwapPlugin, app's main_schedule_label is not Main");
        }

        let (sender, receiver) = crossbeam::channel::unbounded();

        let mut worldswap_subapp = App::empty();
        worldswap_subapp
            .insert_resource(self.clone())
            .insert_resource(SwapCommandSender(sender.clone()))
            .insert_resource(SwapCommandReceiver(receiver))
            .insert_non_send_resource(BackgroundApp { app: None })
            .insert_resource(WorldSwapSubAppState::Running);

        worldswap_subapp.init_schedule(Main);
        app.insert_sub_app(WorldSwapSubApp, SubApp::new(worldswap_subapp, world_swap_extract));

        // Set up the original App's world as a world-swap child.
        // - We include `WorldSwapWindowPlugin` because we don't know yet if this app actually uses windows or not.
        app.add_plugins(WorldSwapWindowPlugin)
            .insert_resource(SwapCommandSender(sender))
            .insert_resource(WorldSwapStatus::Foreground);
    }

    fn cleanup(&self, app: &mut App)
    {
        // Panic if bevy/bevy_render feature is enabled but render subapps haven't been consolidated.
        if app.get_sub_app(RenderApp).is_ok() && app.get_sub_app(RenderExtractApp).is_ok() {
            panic!("failed removing render subapp, WorldSwapPlugin must be added after DefaultPlugins");
        }

        // Add the current world as the foreground app in the world-swap subapp.
        let maybe_render_app = app.remove_sub_app(RenderApp).or_else(|| app.remove_sub_app(RenderExtractApp));
        let worldswap_subapp = app.sub_app_mut(WorldSwapSubApp);

        worldswap_subapp.insert_non_send_resource(ForegroundApp {
            render_app: maybe_render_app,
            // The initial app gets the default background tick rate.
            background_tick_rate: Some(self.background_tick_rate),
        });
    }
}

//-------------------------------------------------------------------------------------------------------------------

/// Plugin group for setting up Bevy plugins in a child world.
///
/// This is a wrapper around Bevy's [`DefaultPlugins`], so you can edit the plugin group in the same way.
/// The [`RenderPlugin`] and [`WinitPlugin`] should **not** be edited.
///
/// Don't use this for setting up your initial app. Use [`WorldSwapPlugin`] and [`DefaultPlugins`] instead.
pub struct ChildDefaultPlugins
{
    pub asset_server: AssetServer,
    pub devices: RenderDevice,
    pub queue: RenderQueue,
    pub adapter_info: RenderAdapterInfo,
    pub adapter: RenderAdapter,
    pub instance: RenderInstance,
    pub synchronous_pipeline_compilation: bool,
}

impl ChildDefaultPlugins
{
    pub fn new(world: &mut World) -> Self
    {
        Self {
            asset_server: world.resource::<AssetServer>().clone(),
            devices: world.resource::<RenderDevice>().clone(),
            queue: world.resource::<RenderQueue>().clone(),
            adapter_info: world.resource::<RenderAdapterInfo>().clone(),
            adapter: world.resource::<RenderAdapter>().clone(),
            instance: world.resource::<RenderInstance>().clone(),
            synchronous_pipeline_compilation: false,
        }
    }
}

impl PluginGroup for ChildDefaultPlugins
{
    fn build(self) -> PluginGroupBuilder
    {
        DefaultPlugins
            .build()
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: ExitCondition::OnAllClosed,
                close_when_requested: true,
            })
            .set(RenderPlugin {
                render_creation: RenderCreation::Manual(
                    self.devices,
                    self.queue,
                    self.adapter_info,
                    self.adapter,
                    self.instance,
                ),
                synchronous_pipeline_compilation: self.synchronous_pipeline_compilation,
            })
            .add_before::<AssetPlugin, InsertAssetServerPlugin>(InsertAssetServerPlugin::new(self.asset_server))
            .add(ChildFocusRepairPlugin)
            .disable::<WinitPlugin>()
            .add(WinitCorePlugin)
            .add(WorldSwapWindowPlugin)
    }
}

//-------------------------------------------------------------------------------------------------------------------
