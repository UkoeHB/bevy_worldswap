
//-------------------------------------------------------------------------------------------------------------------

#[derive(Resource)]
pub(crate) struct ForegroundApp
{
    pub(crate) render_app: Option<SubApp>,
}

//-------------------------------------------------------------------------------------------------------------------

#[derive(Resource)]
pub(crate) struct BackgroundApp
{
    pub(crate) app: Option<WorldSwapApp>,
}

//-------------------------------------------------------------------------------------------------------------------

fn extract_main_world_render_app(main_world: &mut World, subapp_world: &mut World) {
    let Some(render_app) = subapp_world.resource_mut::<ForegroundApp>().render_app else {
        return;
    };

    render_app.extract(main_world);
}

//-------------------------------------------------------------------------------------------------------------------

fn update_background_world(subapp_world: &mut World) {
    let Some(background_app) = subapp_world.resource_mut::<BackgroundApp>().app else {
        return;
    };

    match subapp_world.resource::<WorldSwapPlugin>().background_tick_rate {
        BackgroundTickRate::Never => {
            return
        }
        BackgroundTickRate::EveryTick => {
            background_app.world.run_schedule(Main);
        }
    }
}

//-------------------------------------------------------------------------------------------------------------------

fn apply_pass(main_world: &mut World, subapp_world: &mut World, mut new_app: WorldSwapApp) {

}

//-------------------------------------------------------------------------------------------------------------------

fn apply_fork(main_world: &mut World, subapp_world: &mut World, mut new_app: WorldSwapApp) {

}

//-------------------------------------------------------------------------------------------------------------------

fn apply_swap(main_world: &mut World, subapp_world: &mut World) {

}

//-------------------------------------------------------------------------------------------------------------------

fn apply_join(main_world: &mut World, subapp_world: &mut World) {

}

//-------------------------------------------------------------------------------------------------------------------

/// Label for the world-swap [`SubApp`].
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, AppLabel)]
pub struct WorldSwapSubApp;

pub(crate) fn world_swap_extract(main_world: &mut World, subapp: &mut App) {
    let subapp_world = &mut subapp.world;

    // Extract the main world into its rendering subapp.
    // - We do this inside the world-swap app to ensure rendering extraction synchronizes with swapping worlds. It's
    // also useful for isolating render subapp swaps within the world-swap subapp.
    extract_main_world_render_app(main_world, subapp_world);

    // Update the background world.
    // - Do this first since we want the background world that existed in the just-finished tick to be updated.
    // - Note that any SwapCommands sent by the background world will go to the end of the command queue, so they
    // will take precedence.
    update_background_world(subapp_world);

    // Get and apply the most recent SwapCommand.
    let mut swap_command = None;
    while let Ok(new_swap_command) = subapp.world.resource::<SwapCommandReceiver>().recv() {
        if swap_command.is_some() {
            tracing::warn!("discarding extra swap command");
        }
        swap_command = Some(new_swap_command);
    }

    if let Some(swap_command) = swap_command {
        match swap_command {
            SwapCommand::Pass(new_app) => apply_pass(main_world, subapp_world, new_app),
            SwapCommand::Fork(new_app) => apply_fork(main_world, subapp_world, new_app),
            SwapCommand::Swap => apply_swap(main_world, subapp_world),
            SwapCommand::Join => apply_join(main_world, subapp_world),
        }
    }

/*
    - update cached world (note: do this first, we only want to update cached if it was cached during the last App update)
        - run Main schedule manually
        - if cached world sends AppExit, then discard it
    - listen for 'swap-start' command
        - swap running world with new world
            - move resources from running world to new world
                - make sure SwapCommandSender is inserted to new world
                - TimeReceiver (move into WindowSwapApp struct so the background app doesn't try to receive time)
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
            - swap World instances
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
