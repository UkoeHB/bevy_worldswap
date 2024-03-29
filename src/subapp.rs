use bevy::a11y::AccessibilityRequested;
use bevy::app::{AppExit, AppLabel, SubApp};
use bevy::ecs::entity::EntityHashMap;
use bevy::prelude::*;
use bevy::time::{TimeReceiver, TimeSender};
use bevy::utils::{HashMap, Instant};
use bevy::window::{PrimaryWindow, RawHandleWrapper, WindowCreated};
use bevy::winit::accessibility::{AccessKitAdapters, WinitActionHandlers};
use bevy::winit::{CachedWindow, EventLoopProxy, WinitEvent, WinitSettings, WinitWindows};

use crate::*;

//-------------------------------------------------------------------------------------------------------------------

/// Converts [`AppExit`] events into [`SwapCommand::Join`] commands for foreground worlds IF there is a background
/// world.
fn intercept_app_exit(subapp_world: &World, world: &mut World)
{
    // No interception if there is no background world.
    if subapp_world.non_send_resource::<BackgroundApp>().app.is_none() {
        return;
    }

    // Intercept exit events.
    let Some(mut exit_events) = world.get_resource_mut::<Events<AppExit>>() else { return };
    if exit_events.is_empty() {
        return;
    }

    // Prevent AppExit from continuing into the event loop.
    exit_events.clear();

    // Send join command.
    subapp_world.resource::<SwapCommandSender>().send(SwapCommand::Join);

    tracing::info!("converted AppExit from {:?} into SwapCommand::Join", world.id());
}

//-------------------------------------------------------------------------------------------------------------------

fn can_render(subapp_world: &World, main_world: &World) -> bool
{
    // Don't render if there is no render worker.
    let Some(target) = subapp_world.get_resource::<RenderWorkerTarget>() else { return false };

    // Don't render if waiting for the RenderApp of a previous world to finish its current job.

    // Default value means no worker is running.
    if target.id() == RenderWorkerId::default() {
        return true;
    }
    // Current main world id means the current main world's renderer is running.
    if target.id() == RenderWorkerId::from(main_world) {
        return true;
    }

    // Otherwise, a different world's renderer must be running.
    false
}

//-------------------------------------------------------------------------------------------------------------------

fn extract_main_world_render_app(subapp_world: &mut World, main_world: &mut World)
{
    // Extract the current world and run the render app.
    let Some(render_app) = &mut subapp_world.non_send_resource_mut::<ForegroundApp>().render_app else { return };
    render_app.extract(main_world);
    render_app.run();
}

//-------------------------------------------------------------------------------------------------------------------

fn send_time_to_main_world(subapp_world: &World)
{
    let Some(time_sender) = &subapp_world.non_send_resource::<ForegroundApp>().time_sender else { return };
    let _ = time_sender.0.send(Instant::now());
}

//-------------------------------------------------------------------------------------------------------------------

fn get_background_tick_rate(
    default_tick_rate: BackgroundTickRate,
    background_tick_rate_of_app: Option<BackgroundTickRate>,
) -> BackgroundTickRate
{
    background_tick_rate_of_app.unwrap_or(default_tick_rate)
}

//-------------------------------------------------------------------------------------------------------------------

fn update_background_world(subapp_world: &mut World) -> bool
{
    if *subapp_world.resource::<WorldSwapSubAppState>() == WorldSwapSubAppState::Exiting {
        return true;
    }

    let close_on_exit = subapp_world.resource::<WorldSwapPlugin>().abort_on_background_exit;
    let default_tick_rate = subapp_world.resource::<WorldSwapPlugin>().background_tick_rate;
    let Some(background_app) = &mut subapp_world.non_send_resource_mut::<BackgroundApp>().app else {
        return false;
    };

    // Detect AppExit in the background world.
    // - Do this before updating the background world in case AppExit was sent in a previous update.
    if !background_app.world.resource::<Events<AppExit>>().is_empty() {
        return close_on_exit;
    }

    // Update the background app.
    match get_background_tick_rate(default_tick_rate, background_app.background_tick_rate) {
        BackgroundTickRate::Never { .. } => (),
        BackgroundTickRate::EveryTick => {
            background_app.world.run_schedule(Main);
        }
    }

    // Check if AppExit was emitted during the update.
    if !background_app.world.resource::<Events<AppExit>>().is_empty() {
        return close_on_exit;
    }

    false
}

//-------------------------------------------------------------------------------------------------------------------

fn transfer_windows(main_world: &mut World, new_world: &mut World)
{
    // Make sure the new world uses the same accessibility toggle, since it is embedded in accessibility nodes for
    // existing windows.
    if let Some(accessibility_toggle) = main_world.get_resource::<AccessibilityRequested>() {
        new_world.insert_resource(accessibility_toggle.clone());
    }

    // Extract WinitWindows.
    let Some(mut main_windows) = main_world.remove_non_send_resource::<WinitWindows>() else { return };
    let mut new_windows = new_world
        .remove_non_send_resource::<WinitWindows>()
        .expect("if the main world has WinitWindows, the new world should too");

    // Validate that the new world did not create any windows.
    if new_windows.windows.len() > 0 {
        panic!("a world that isn't in the foreground created windows");
    }

    // Move winit windows to the new world.
    new_windows.windows = std::mem::replace(&mut main_windows.windows, HashMap::default());

    // Despawn window entities in the new world if they don't have windows.
    for (entity, window_id) in new_windows.entity_to_winit.iter() {
        if new_windows.windows.contains_key(window_id) {
            continue;
        }

        new_world.despawn(*entity);
        new_windows.winit_to_entity.remove(window_id);

        // NOTE: WindowClosed events don't need to be sent, as they will be sent automatically by ChildWinitPlugin
    }

    // Synchronize window entities.
    for (window_id, _) in new_windows.windows.iter() {
        // Access components from the main world.
        // - We REMOVE RawHandleWrapper so the main world can be render-extracted without rendering anything.
        let Some(main_entity) = main_windows.winit_to_entity.get(window_id) else {
            tracing::error!("main world is missing an entity for window id {:?}", window_id);
            continue;
        };
        let maybe_raw_handle_wrapper = main_world.entity_mut(*main_entity).take::<RawHandleWrapper>();
        let Some(window) = main_world.get::<Window>(*main_entity) else {
            tracing::error!(
                "main world window entity {:?} is missing a Window component for {:?}",
                main_entity,
                window_id
            );
            continue;
        };
        let Some(cached_window) = main_world.get::<CachedWindow>(*main_entity) else {
            tracing::error!(
                "main world window entity {:?} is missing a CachedWindow component for {:?}",
                main_entity,
                window_id
            );
            continue;
        };
        let maybe_primary = main_world.get::<PrimaryWindow>(*main_entity);

        // Handle existing vs new window entities.
        if let Some(new_entity) = new_windows.winit_to_entity.get(window_id) {
            // Clone window components into existing window entities in the new world.
            let mut new_entity = new_world.get_entity_mut(*new_entity).unwrap();
            new_entity.insert(window.clone());

            // Synchronize RawHandleWrapper component.
            if let Some(raw_handle_wrapper) = maybe_raw_handle_wrapper {
                new_entity.insert(raw_handle_wrapper);
            } else {
                new_entity.remove::<RawHandleWrapper>();
            }

            // Synchronize PrimaryWindow component.
            if let Some(primary) = maybe_primary {
                new_entity.insert(*primary);
            } else {
                new_entity.remove::<PrimaryWindow>();
            }

            // NOTE: WindowResized events don't need to be sent, as they will be sent automatically by
            // ChildWinitPlugin
        } else {
            // Spawn new window entities in the new world to match unknown window ids.
            let mut entity_cmds = new_world.spawn((window.clone(), cached_window.clone()));
            if let Some(raw_handle_wrapper) = maybe_raw_handle_wrapper {
                entity_cmds.insert(raw_handle_wrapper);
            }
            if let Some(primary) = maybe_primary {
                entity_cmds.insert(*primary);
            }

            let entity_id = entity_cmds.id();
            new_windows.winit_to_entity.insert(*window_id, entity_id);

            // Send WindowCreated event to the new world.
            // - We must do this manually because we bypass the Bevy code path that emits these events, because
            //   that
            // code path actually creates new OS windows.
            // - Note that the WinitEvent WONT synchronize with other window events, which is unfortunate and COULD
            // cause bugs for someone.
            let event = WindowCreated { window: entity_id };
            new_world.send_event(event.clone());
            new_world.send_event(WinitEvent::WindowCreated(event));
        }
    }

    // Rebuild entity_to_winit map.
    new_windows.entity_to_winit.clear();
    for (window_id, entity) in new_windows.winit_to_entity.iter() {
        new_windows.entity_to_winit.insert(*entity, *window_id);
    }
    debug_assert_eq!(new_windows.entity_to_winit.len(), new_windows.windows.len());

    // Transfer AccessKitAdapters to the new world.
    if let Some(mut access_kit) = main_world.remove_non_send_resource::<AccessKitAdapters>() {
        let mut new_access_kit = EntityHashMap::default();
        for (entity, adapter) in access_kit.drain() {
            let Some(new_entity) = map_winit_window_entities(&main_windows, &new_windows, entity) else {
                continue;
            };
            new_access_kit.insert(new_entity, adapter);
        }
        new_world.insert_non_send_resource(AccessKitAdapters(new_access_kit));
    }

    // Transfer WinitActionHandlers to the new world.
    if let Some(mut action_handlers) = main_world.remove_resource::<WinitActionHandlers>() {
        let mut new_action_handlers = EntityHashMap::default();
        for (entity, handler) in action_handlers.drain() {
            let Some(new_entity) = map_winit_window_entities(&main_windows, &new_windows, entity) else {
                continue;
            };
            new_action_handlers.insert(new_entity, handler);
        }
        new_world.insert_resource(WinitActionHandlers(new_action_handlers));
    }

    // Return WinitWindows.
    main_world.insert_non_send_resource(main_windows);
    new_world.insert_non_send_resource(new_windows);
}

//-------------------------------------------------------------------------------------------------------------------

fn drain_cached_window_events(main_world: &mut World, new_world: &mut World)
{
    // Get WinitWindows for entity mapping.
    let Some(mut main_windows) = main_world.remove_non_send_resource::<WinitWindows>() else { return };
    let mut new_windows = new_world
        .remove_non_send_resource::<WinitWindows>()
        .expect("if main world has WinitWindows, new worlds should too");

    // Send window events
    let mut main_window_events = main_world.resource_mut::<WindowEventCache>();
    main_window_events.dispatch(&mut main_windows, &mut new_windows, new_world);

    // Put WinitWindows back.
    main_world.insert_non_send_resource(main_windows);
    new_world.insert_non_send_resource(new_windows);
}

//-------------------------------------------------------------------------------------------------------------------

fn prepare_world_swap(subapp_world: &mut World, main_world: &mut World, new_world: &mut World)
{
    // SwapCommandSender is needed in the new world.
    new_world.insert_resource(subapp_world.resource::<SwapCommandSender>().clone());

    // Connect the new world to the winit event loop.
    if new_world.get_non_send_resource::<EventLoopProxy>().is_none() {
        if let Some(event_loop_proxy) = main_world.get_non_send_resource::<EventLoopProxy>() {
            new_world.insert_non_send_resource(event_loop_proxy.clone());
        }
    }

    // Set the new world's winit settings IF the new world hasn't already specified it.
    // - Users may manually insert different WinitSettings for each world (e.g. WinitSettings::desktop_app for
    //   menu,
    // WinitSettings::game for game).
    if let Some(winit_settings) = main_world.get_resource::<WinitSettings>() {
        if !new_world.contains_resource::<WinitSettings>() {
            new_world.insert_resource(winit_settings.clone());
        }
    }

    // Update window entities in the new world.
    transfer_windows(main_world, new_world);

    // Drain cached window events into the new world.
    // - This must be done after updating window entities in the new world, so event entities can be mapped
    //   properly.
    // - Note that window events will ping-pong when swapping worlds since we don't have a way to know if a window
    //   event
    // is ping-ponged or emitted by the app. This should at most cause systems that react to those events to run
    // redundantly every time you swap.
    //todo: fix event ping-ponging? can cache last-seen event values in WindowEventCache, and don't dispatch
    // events if the values won't change
    drain_cached_window_events(main_world, new_world);
}

//-------------------------------------------------------------------------------------------------------------------

fn take_background_app(subapp_world: &mut World) -> Option<WorldSwapApp>
{
    let mut background_app = subapp_world.non_send_resource_mut::<BackgroundApp>().app.take()?;

    // Restart the background world's virtual clock if it was paused.
    if background_app.paused_by_tick_policy {
        background_app.world.resource_mut::<Time<Virtual>>().unpause();
        background_app.paused_by_tick_policy = false;
    }

    Some(background_app)
}

//-------------------------------------------------------------------------------------------------------------------

fn swap_worlds(subapp_world: &mut World, main_world: &mut World, mut new_app: WorldSwapApp) -> WorldSwapApp
{
    // Swap worlds.
    std::mem::swap(main_world, &mut new_app.world);

    // Swap background tick rates.
    let new_background_tick_rate = new_app.background_tick_rate.take();
    new_app.background_tick_rate =
        subapp_world.non_send_resource_mut::<ForegroundApp>().background_tick_rate.take();
    subapp_world.non_send_resource_mut::<ForegroundApp>().background_tick_rate = new_background_tick_rate;

    // Note: `paused_by_tick_policy` is handled by `take_background_app` and `add_app_to_background`.
    debug_assert!(!new_app.paused_by_tick_policy);

    // Swap time senders.
    let new_time_sender = new_app.time_sender.take();
    new_app.time_sender = subapp_world.non_send_resource_mut::<ForegroundApp>().time_sender.take();
    subapp_world.non_send_resource_mut::<ForegroundApp>().time_sender = new_time_sender;

    // Swap time receivers.
    if let Some(time_receiver) = new_app.time_receiver.take() {
        main_world.insert_resource(time_receiver);
    }
    new_app.time_receiver = new_app.world.remove_resource::<TimeReceiver>();

    // Swap render apps.
    let new_render_app = new_app.render_app.take();
    new_app.render_app = subapp_world.non_send_resource_mut::<ForegroundApp>().render_app.take();
    subapp_world.non_send_resource_mut::<ForegroundApp>().render_app = new_render_app;

    // Update statuses.
    main_world.insert_resource(WorldSwapStatus::Foreground);
    new_app.world.insert_resource(WorldSwapStatus::Suspended);

    new_app
}

//-------------------------------------------------------------------------------------------------------------------

fn freeze_time_in_background(subapp_world: &World, background_tick_rate_of_app: Option<BackgroundTickRate>)
    -> bool
{
    let rate = get_background_tick_rate(
        subapp_world.resource::<WorldSwapPlugin>().background_tick_rate,
        background_tick_rate_of_app,
    );
    let BackgroundTickRate::Never { freeze_time } = rate else { return false };

    freeze_time
}

//-------------------------------------------------------------------------------------------------------------------

fn add_app_to_background(subapp_world: &mut World, mut background_app: WorldSwapApp)
{
    // Prep background status.
    background_app.world.insert_resource(WorldSwapStatus::Background);

    // Pause the background app if necessary.
    background_app.paused_by_tick_policy = false;
    if freeze_time_in_background(subapp_world, background_app.background_tick_rate) {
        let time = background_app.world.resource_mut::<Time<Virtual>>();

        if !time.is_paused() {
            background_app.world.resource_mut::<Time<Virtual>>().pause();
            background_app.paused_by_tick_policy = true;
        }
    }

    // Insert the background app.
    let prev_background = subapp_world.non_send_resource_mut::<BackgroundApp>().app.replace(background_app);
    assert!(prev_background.is_none());
}

//-------------------------------------------------------------------------------------------------------------------

fn handle_swap_pass_recovery(subapp_world: &mut World, main_world: &mut World, passing_app: WorldSwapApp)
{
    let Some(recovery_fn) = subapp_world.resource::<WorldSwapPlugin>().swap_pass_recovery else { return };

    (recovery_fn)(main_world, passing_app);
}

//-------------------------------------------------------------------------------------------------------------------

fn handle_swap_join_recovery(subapp_world: &mut World, main_world: &mut World, joined_app: WorldSwapApp)
{
    let Some(recovery_fn) = subapp_world.resource::<WorldSwapPlugin>().swap_join_recovery else { return };

    (recovery_fn)(main_world, joined_app);
}

//-------------------------------------------------------------------------------------------------------------------

fn apply_pass(subapp_world: &mut World, main_world: &mut World, mut new_app: WorldSwapApp)
{
    tracing::info!("foreground control passed from {:?} to {:?}; recovering or dropping {:?}",
        main_world.id(), new_app.world.id(), main_world.id());

    // Prepare the new world.
    prepare_world_swap(subapp_world, main_world, &mut new_app.world);

    // Force-render the foreground after removing windows.
    extract_main_world_render_app(subapp_world, main_world);

    // Swap the previous world for the new world.
    let prev_app = swap_worlds(subapp_world, main_world, new_app);

    // The previous world is passed to the swap-pass-recovery callback, otherwise dropped.
    handle_swap_pass_recovery(subapp_world, main_world, prev_app);
}

//-------------------------------------------------------------------------------------------------------------------

fn apply_fork(subapp_world: &mut World, main_world: &mut World, mut new_app: WorldSwapApp)
{
    if subapp_world.non_send_resource::<BackgroundApp>().app.is_some() {
        panic!("SwapCommand::Fork is not allowed when there is already a world in the background");
    }

    tracing::info!("{:?} forked, now {:?} is foreground and {:?} is background",
        main_world.id(), new_app.world.id(), main_world.id());

    // Prepare the new world.
    prepare_world_swap(subapp_world, main_world, &mut new_app.world);

    // Force-render the foreground after removing windows.
    extract_main_world_render_app(subapp_world, main_world);

    // Swap the previous world for the new world.
    let prev_app = swap_worlds(subapp_world, main_world, new_app);

    // Put the previous world in the background.
    add_app_to_background(subapp_world, prev_app);
}

//-------------------------------------------------------------------------------------------------------------------

fn apply_swap(subapp_world: &mut World, main_world: &mut World)
{
    if subapp_world.non_send_resource::<BackgroundApp>().app.is_none() {
        panic!("SwapCommand::Swap is only allowed when there is a world in the background");
    }

    let mut background_app = take_background_app(subapp_world).unwrap();
    tracing::info!("{:?} swapped, now {:?} is foreground and {:?} is background",
        main_world.id(), background_app.world.id(), main_world.id());

    // Prepare the background world for entering the foreground.
    prepare_world_swap(subapp_world, main_world, &mut background_app.world);

    // Force-render the foreground after removing windows.
    extract_main_world_render_app(subapp_world, main_world);

    // Swap the previous world for the background world.
    let prev_app = swap_worlds(subapp_world, main_world, background_app);

    // Put the previous world in the background.
    add_app_to_background(subapp_world, prev_app);
}

//-------------------------------------------------------------------------------------------------------------------

fn apply_join(subapp_world: &mut World, main_world: &mut World)
{
    let Some(mut background_app) = take_background_app(subapp_world) else {
        panic!("SwapCommand::Join is only allowed when there is a world in the background");
    };
    tracing::info!("{:?} joined, now {:?} is foreground; recovering or dropping {:?}",
        main_world.id(), background_app.world.id(), main_world.id());

    // Prepare the background world for entering the foreground..
    prepare_world_swap(subapp_world, main_world, &mut background_app.world);

    // Force-render the foreground after removing windows.
    extract_main_world_render_app(subapp_world, main_world);

    // Swap the previous world for the background world.
    let prev_app = swap_worlds(subapp_world, main_world, background_app);

    // The previous world is passed to the swap-join-recovery callback, otherwise dropped.
    handle_swap_join_recovery(subapp_world, main_world, prev_app);
}

//-------------------------------------------------------------------------------------------------------------------

pub(crate) struct ForegroundApp
{
    pub(crate) render_app: Option<SubApp>,
    pub(crate) background_tick_rate: Option<BackgroundTickRate>,
    pub(crate) time_sender: Option<TimeSender>,
}

//-------------------------------------------------------------------------------------------------------------------

pub(crate) struct BackgroundApp
{
    pub(crate) app: Option<WorldSwapApp>,
}

//-------------------------------------------------------------------------------------------------------------------

#[derive(Resource, Copy, Clone, Eq, PartialEq)]
pub(crate) enum WorldSwapSubAppState
{
    Running,
    Exiting,
}

//-------------------------------------------------------------------------------------------------------------------

/// Label for the world-swap [`SubApp`].
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, AppLabel)]
pub(crate) struct WorldSwapSubApp;

pub(crate) fn world_swap_extract(main_world: &mut World, subapp: &mut App)
{
    let subapp_world = &mut subapp.world;

    // Intercept AppExit events from the main world and convert them to SwapCommand::Join commands if possible.
    // - We do this here instead of as a system in the world to ensure *all* AppExit events are captured.
    intercept_app_exit(subapp_world, main_world);

    // Get any commands sent by the main world.
    let mut swap_command = None;
    while let Ok(new_swap_command) = subapp_world.resource::<SwapCommandReceiver>().try_recv() {
        if swap_command.is_some() {
            tracing::warn!("discarding extra swap command");
        }
        swap_command = Some(new_swap_command);
    }

    // Apply the most recent SwapCommand.
    // - This will force-render the foreground world after removing windows, which ensures the foreground world
    // is 'fully updated' in case it expects a strict 'update - extract' sequence. We don't display the foreground
    // world's last frame (i.e. we render after removing windows) because it may contain visual effects of the swap
    // (e.g. button/state changes) that should only be shown after swapping back.
    let swapped = swap_command.is_some();
    if let Some(swap_command) = swap_command {
        match swap_command {
            SwapCommand::Pass(new_app) => apply_pass(subapp_world, main_world, new_app),
            SwapCommand::Fork(new_app) => apply_fork(subapp_world, main_world, new_app),
            SwapCommand::Swap => apply_swap(subapp_world, main_world),
            SwapCommand::Join => apply_join(subapp_world, main_world),
        }
    }

    // Extract the main world into its rendering subapp.
    // - We do NOT extract if we are waiting for a pipelined RenderApp from a previous world to finish its current
    //   job.
    if !swapped && can_render(subapp_world, main_world) {
        extract_main_world_render_app(subapp_world, main_world);
    } else if !swapped {
        // If we didn't extract, then we need to send time manually to the main world otherwise Bevy logs a
        // warning.
        // - We do NOT send time to the just-swapped-in world because it did not yet update after being hooked back
        //   up
        // to TimeReceiver. Note that without `!swapped` the app will freeze when swapping back to the background
        // world.
        send_time_to_main_world(subapp_world);
    }

    // If we swapped this tick, then skip the background update since the background world was just updated in the
    // foreground.
    if swapped {
        return;
    }

    // Update the background world.
    // - Do this last so rendering the foreground world is scheduled as soon as possible.
    // - Note that any SwapCommands sent by the background world will go to the beginning of the command queue, so
    // foreground commands will take precedence.
    let should_exit = update_background_world(subapp_world);

    if should_exit {
        main_world.send_event(AppExit);
        subapp_world.insert_resource(WorldSwapSubAppState::Exiting);
    }
}

//-------------------------------------------------------------------------------------------------------------------
