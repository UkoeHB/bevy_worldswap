
//-------------------------------------------------------------------------------------------------------------------
//-------------------------------------------------------------------------------------------------------------------

/// Converts [`AppExit`] events into [`SwapCommand::Join`] commands for foreground worlds IF there is a background world.
fn intercept_app_exit(subapp_world: &World, world: &mut World) {
    // No interception if there is no background world.
    if subapp_world.resource::<BackgroundApp>().app.is_none() {
        return;
    }

    // Intercept exit events.
    let exit_events = world.resource_mut::<Events<AppExit>>();
    if exit_events.is_empty() {
        return;
    }
    exit_events.clear();

    // Send join command.
    subapp_world.resource::<SwapCommandSender>().send(SwapCommand::Join);
}

//-------------------------------------------------------------------------------------------------------------------
//-------------------------------------------------------------------------------------------------------------------

fn extract_main_world_render_app(subapp_world: &mut World, main_world: &mut World) {
    let Some(render_app) = subapp_world.resource_mut::<ForegroundApp>().render_app else {
        return;
    };

    render_app.extract(main_world);
}

//-------------------------------------------------------------------------------------------------------------------
//-------------------------------------------------------------------------------------------------------------------

fn get_background_tick_rate(
    default_tick_rate: BackgroundTickRate,
    background_tick_rate_of_app: Option<BackgroundTickRate>
) -> BackgroundTickRate {
    background_tick_rate_of_app.unwrap_or(default_tick_rate);
}

//-------------------------------------------------------------------------------------------------------------------
//-------------------------------------------------------------------------------------------------------------------

fn update_background_world(subapp_world: &mut World) -> bool {
    if *subapp_world.resource::<WorldSwapSubAppState>() == WorldSwapSubAppState::Exiting {
        return true;
    }

    let close_on_exit = subapp_world.resource::<WorldSwapPlugin>().abort_on_background_exit;
    let default_tick_rate = subapp_world.resource::<WorldSwapPlugin>().background_tick_rate;
    let Some(background_app) = subapp_world.resource_mut::<BackgroundApp>().app else {
        return false;
    };

    // Detect AppExit in the background world.
    // - Do this before updating the background world in case AppExit was sent in a previous update.
    if !background_app.world.resource::<Events<AppExit>>().is_empty() {
        return close_on_exit;
    }

    // Update the background app.
    match get_background_tick_rate(default_tick_rate, background_app.background_tick_rate) {
        BackgroundTickRate::Never{ .. } => (),
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
//-------------------------------------------------------------------------------------------------------------------

fn transfer_window_entities(main_world: &mut World, new_world: &mut World) {

/*
- Update window entities in new world, using WinitWindows maps from each app (use window ids for cross-map)
    - Must MOVE winit::window::Window values between WinitWindows instances, since those actually store
    the winit window instance (bevy Window is just a config transport).
    - Despawn cached new-world entities if their matching running-world entities don't exist.
    - Spawn new-world entities if running-world entities don't exist in the map.
        - Manually add these to WinitWindows in new world, since we don't want to use create_windows(), which
        actually spawns new winit windows.
        - Manually add RawHandleWrapper and CachedWindow components
        - Add PrimaryWindow component if necessary
        - send WindowCreated event
    - Clone Window components into existing entities.
    - Update AccessKitAdapters, WinitActionHandlers
*/

}

//-------------------------------------------------------------------------------------------------------------------
//-------------------------------------------------------------------------------------------------------------------

fn drain_cached_window_events(main_world: &mut World, new_world: &mut World) {
    // Get WinitWindows for entity mapping.
    let Some(main_windows) = main_world.remove_resource::<WinitWindows>() else {
        return;
    };
    let new_windows = new_world.remove_resource::<WinitWindows>()
        .expect("if main world has WinitWindows, new worlds should too");

    // Send window events
    let main_window_events = main_world.resource_mut::<WindowEventCache>();
    main_window_events.dispatch(&mut main_windows, &mut new_windows, new_world);

    // Put WinitWindows back.
    main_world.insert_resource(main_windows);
    new_world.insert_resource(new_windows);
}

//-------------------------------------------------------------------------------------------------------------------
//-------------------------------------------------------------------------------------------------------------------

fn prepare_world_swap(subapp_world: &mut World, main_world: &mut World, new_world: &mut World) {
    // SwapCommandSender is needed in the new world.
    new_world,.insert_resource(subapp_world.resource::<SwapCommandSender>().clone());

    // Connect the new world to the winit event loop.
    if new_world.get_non_send_resource::<EventLoopProxy>().is_none() {
        if let Some(event_loop_proxy) = main_world.get_non_send_resource::<EventLoopProxy>() {
            new_world.insert_non_send_resource(event_loop_proxy.clone());
        }
    }

    // Set the new world's winit settings IF the new world hasn't already specified it.
    // - Users may manually insert different WinitSettings for each world (e.g. WinitSettings::desktop_app for menu,
    // WinitSettings::game for game).
    if let Some(winit_settings) = main_world.get_resource::<WinitSettings>() {
        if !new_world.contains_resource::<WinitSettings>() {
            new_world.insert_resource(winit_settings.clone());
        }
    }

    // Update window entities in the new world.
    transfer_window_entities(main_world, new_world);

    // Drain cached window events into the new world.
    // - This must be done after updating window entities in the new world, so event entities can be mapped properly.
    // - Note that window events will ping-pong when swapping worlds since we don't have a way to know if a window event
    // is ping-ponged or emitted by the app. This should at most cause systems that react to those events to run
    // redundantly every time you swap.
    //todo: fix event ping-ponging? can cache last-seen event values in WindowEventCache, and don't dispatch events if the
    // values won't change
    drain_cached_window_events(main_world, new_world);
}

//-------------------------------------------------------------------------------------------------------------------
//-------------------------------------------------------------------------------------------------------------------

fn take_background_app(subapp_world: &mut World) -> Option<WorldSwapApp> {
    let mut background_app = subapp_world.resource_mut::<BackgroundApp>().app.take()?;
    
    // Restart the background world's virtual clock if it was paused.
    if background_app.paused_by_tick_policy {
        background_app.world.resource_mut::<Time<Virtual>>().unpause();
        background_app.paused_by_tick_policy = false;
    }

    Some(background_app)
}

//-------------------------------------------------------------------------------------------------------------------
//-------------------------------------------------------------------------------------------------------------------

fn swap_worlds(subapp_world: &mut World, main_world: &mut World, mut new_app: WorldSwapApp) -> WorldSwapApp {
    // Swap worlds.
    std::mem::swap(main_world, &mut new_app.world);

    // Swap background tick rates.
    let new_background_tick_rate = new_app.background_tick_rate.take();
    new_app.background_tick_rate = subapp_world.resource_mut::<ForegroundApp>().background_tick_rate.take();
    *subapp_world.resource_mut::<ForegroundApp>().background_tick_rate = new_background_tick_rate;

    // Swap time receivers.
    if let Some(time_receiver) = new_app.time_receiver.take() {
        main_world.insert_resource(time_receiver);
    }
    new_app.time_receiver = new_app.world.remove_resource::<TimeReceiver>();

    // Swap render apps.
    let new_render_app = new_app.render_app.take();
    new_app.render_app = subapp_world.resource_mut::<ForegroundApp>().render_app.take();
    *subapp_world.resource_mut::<ForegroundApp>().render_app = new_render_app;

    // Update statuses.
    *main_world.insert_resource(WorldSwapStatus::Foreground);
    *new_app.world.insert_resource(WorldSwapStatus::Suspended);

    new_app
}

//-------------------------------------------------------------------------------------------------------------------
//-------------------------------------------------------------------------------------------------------------------

fn freeze_time_in_background(subapp_world: &World, background_tick_rate_of_app: Option<BackgroundTickRate>) {
    let rate = get_background_tick_rate(
        subapp_world.resource::<WorldSwapPlugin>().background_tick_rate,
        background_tick_rate_of_app
    );
    let BackgroundTickRate::Never{ freeze_time } = rate else {
        return false;
    };

    freeze_time
}

//-------------------------------------------------------------------------------------------------------------------
//-------------------------------------------------------------------------------------------------------------------

fn add_app_to_background(subapp_world: &mut World, mut background_app: WorldSwapApp) {
    // Prep background status.
    *background_app.world.insert_resource(WorldSwapStatus::Background);

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
    let prev_background = *subapp_world.resource_mut::<BackgroundApp>().app.replace(background_app);
    assert!(prev_background.is_none());
}

//-------------------------------------------------------------------------------------------------------------------
//-------------------------------------------------------------------------------------------------------------------

fn handle_swap_join_recovery(subapp_world: &mut World, main_world: &mut World, joined_app: WorldSwapApp) {
    let Some(recovery_fn) = subapp_world.resource::<WorldSwapPlugin>().swap_join_recovery else {
        return;
    };

    (*recovery_fn)(main_world, joined_app);
}

//-------------------------------------------------------------------------------------------------------------------
//-------------------------------------------------------------------------------------------------------------------

fn apply_pass(subapp_world: &mut World, main_world: &mut World, mut new_app: WorldSwapApp) {
    tracing::info!("foreground control passed from world {:?} to world {:?}, world {:?} has been dropped",
        main_world.id(), new_app.world.id(), main_world.id());

    // Prepare the new world.
    prepare_world_swap(subapp_world, main_world, &mut new_app.world);

    // Swap the previous world for the new world.
    let _prev_app = swap_worlds(subapp_world, main_world, new_app);

    // The previous world is dropped.
}

//-------------------------------------------------------------------------------------------------------------------
//-------------------------------------------------------------------------------------------------------------------

fn apply_fork(subapp_world: &mut World, main_world: &mut World, mut new_app: WorldSwapApp) {
    if subapp_world.resource::<BackgroundApp>().app.is_some() {
        panic!("SwapCommand::Fork is not allowed when there is already a world in the background");
    }

    tracing::info!("world {:?} forked, now world {:?} is in the foreground and world {:?} is in the background",
        main_world.id(), new_app.world.id(), main_world.id());

    // Prepare the new world.
    prepare_world_swap(subapp_world, main_world, &mut new_app.world);

    // Swap the previous world for the new world.
    let prev_app = swap_worlds(subapp_world, main_world, new_app);

    // Put the previous world in the background.
    add_app_to_background(subapp_world, prev_app);
}

//-------------------------------------------------------------------------------------------------------------------
//-------------------------------------------------------------------------------------------------------------------

fn apply_swap(subapp_world: &mut World, main_world: &mut World) {
    if subapp_world.resource::<BackgroundApp>().app.is_none() {
        panic!("SwapCommand::Swap is only allowed when there is a world in the background");
    }

    let mut background_app = take_background_app(subapp_world).unwrap();
    tracing::info!("world {:?} swapped, now world {:?} is in the foreground and world {:?} is in the background",
        main_world.id(), background_app.world.id(), main_world.id());

    // Prepare the background world for entering the foreground.
    prepare_world_swap(subapp_world, main_world, &mut background_app.world);

    // Swap the previous world for the background world.
    let prev_app = swap_worlds(subapp_world, main_world, background_app);

    // Put the previous world in the background.
    add_app_to_background(subapp_world, prev_app);
}

//-------------------------------------------------------------------------------------------------------------------
//-------------------------------------------------------------------------------------------------------------------

fn apply_join(subapp_world: &mut World, main_world: &mut World) {
    let Some(mut background_app) = take_background_app(subapp_world) else {
        panic!("SwapCommand::Join is only allowed when there is a world in the background");
    };
    tracing::info!("world {:?} joined, now world {:?} is in the foreground and world {:?} has been recovered or dropped",
        main_world.id(), background_app.world.id(), main_world.id());

    // Prepare the background world for entering the foreground..
    prepare_world_swap(subapp_world, main_world, &mut background_app.world);

    // Swap the previous world for the background world.
    let prev_app = swap_worlds(subapp_world, main_world, background_app);

    // The previous world is passed to the swap-join-recovery callback, otherwise dropped.
    handle_swap_join_recovery(subapp_world, main_world, prev_app);
}

//-------------------------------------------------------------------------------------------------------------------
//-------------------------------------------------------------------------------------------------------------------

#[derive(Resource)]
pub(crate) struct ForegroundApp
{
    pub(crate) render_app: Option<SubApp>,
    pub(crate) background_tick_rate: Option<BackgroundTickRate>,
}

//-------------------------------------------------------------------------------------------------------------------

#[derive(Resource)]
pub(crate) struct BackgroundApp
{
    pub(crate) app: Option<WorldSwapApp>,
}

//-------------------------------------------------------------------------------------------------------------------

#[derive(Resource, Copy, Clone, Eq, PartialEq)]
pub(crate) enum WorldSwapSubAppState {
    Running,
    Exiting,
}

//-------------------------------------------------------------------------------------------------------------------

/// Label for the world-swap [`SubApp`].
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, AppLabel)]
pub struct WorldSwapSubApp;

pub(crate) fn world_swap_extract(main_world: &mut World, subapp: &mut App) {
    let subapp_world = &mut subapp.world;

    // Intercept AppExit events from the main world and convert them to SwapCommand::Join commands if possible.
    // - We do this here instead of as a system in the world to ensure *all* AppExit events are captured.
    intercept_app_exit(subapp_world, main_world);

    // Extract the main world into its rendering subapp.
    // - We do this inside the world-swap app to ensure rendering extraction synchronizes with swapping worlds. It's
    // also useful for isolating render subapp swaps within the world-swap subapp.
    extract_main_world_render_app(subapp_world, main_world);

    // Update the background world.
    // - Do this first since we want the background world that existed in the just-finished tick to be updated.
    // - Note that any SwapCommands sent by the background world will go to the end of the command queue, so they
    // will take precedence.
    let should_exit = update_background_world(subapp_world);

    if should_exit {
        main_world.send_event(AppExit);
        *subapp_world.insert_resource(WorldSwapSubAppState::Exiting);
    }

    // Get and apply the most recent SwapCommand.
    let mut swap_command = None;
    while let Ok(new_swap_command) = subapp_world.resource::<SwapCommandReceiver>().recv() {
        if swap_command.is_some() {
            tracing::warn!("discarding extra swap command");
        }
        swap_command = Some(new_swap_command);
    }

    if let Some(swap_command) = swap_command {
        match swap_command {
            SwapCommand::Pass(new_app) => apply_pass(subapp_world, main_world, new_app),
            SwapCommand::Fork(new_app) => apply_fork(subapp_world, main_world, new_app),
            SwapCommand::Swap => apply_swap(subapp_world, main_world),
            SwapCommand::Join => apply_join(subapp_world, main_world),
        }
    }

/*
- note: need to update WorldSwapState correctly
- update cached world (note: do this first, we only want to update cached if it was cached during the last App update)
    - run Main schedule manually
    - if cached world sends AppExit, then discard it
- listen for 'swap-start' command
    - swap running world with new world
        - move resources from running world to new world
            - make sure SwapCommandSender is inserted to new world
            - EventLoopProxy (clone)
            - WinitSettings (clone)
            - Update window entities in new world, using WinitWindows maps from each app (use window ids for cross-map)
                - Must MOVE winit::window::Window values between WinitWindows instances, since those actually store
                the winit window instance (bevy Window is just a config transport).
                - Despawn cached new-world entities if their matching running-world entities don't exist.
                - Spawn new-world entities if running-world entities don't exist in the map.
                    - Manually add these to WinitWindows in new world, since we don't want to use create_windows(), which
                    actually spawns new winit windows.
                    - Manually add RawHandleWrapper and CachedWindow components
                    - Add PrimaryWindow component if necessary
                    - send WindowCreated event
                - Clone Window components into existing entities.
                - Update AccessKitAdapters, WinitActionHandlers
            - Issue cached window events in the new world (need to map window entities)
                - WindowBackendScaleFactorChanged (most recent for a specific window)
                - WindowScaleFactorChanged (most recent for a specific window)
                - WindowThemeChanged (most recent for a specific window)
        - swap
            - swap World instances
            - TimeReceiver (move into WindowSwapApp struct so the background app doesn't try to receive time)
            - swap RenderApp/RenderExtractApp SubApp instances
        - save swapped-out world's WorldSwapApp for swapping back in
    - if no world cached
        - disable the running world
            - set flag in WorldSwapInfo resource
        - cache it
- listen for 'swap-end' command
    - ignore if current running world's ID != swap-end id (this way you can windowpass between secondary worlds without
    needing to deal with running the swap-end recovery callback *and then* running another callback to pass resources
    from the main world to the new running world; you can do a chained sequence that goes 'swap-start' -> 'swap-start' ->
    'swap-end', ending back in the main world)
    - exit if there is no cached world (implies cached world shut down)
    - swap running world with cached world
        - move resources from running world to cached world
            - TimeReceiver (move back into cached world)
            - Update window entities in cached world
            - Issue cached window events in the cached world
        - re-enable cached world
            - unset flag in WorldSwapInfo resource
        - return the cached RenderApp/RenderExtractApp SubApp, discard the removed one
        - run 'swap-end recovery' callback on the two worlds
            - this moves the running world into the callback, so you can trivially 'pause' the running world and then
            restart it with another 'swap-start'
*/
}

//-------------------------------------------------------------------------------------------------------------------
